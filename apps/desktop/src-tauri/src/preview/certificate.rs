//! Certificate preview: `.pem`, `.crt`, `.cer` and `.der` are parsed with
//! `x509-parser` into readable fields. Chains list every certificate. Read-only.

use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Serialize;
use tauri::State;
use x509_parser::prelude::{FromDer, GeneralName, X509Certificate};

use crate::app::db::Database;
use crate::preview::source::{decode_text, read_bytes, SOURCE_MAX_BYTES};
use crate::{network, remote};

#[derive(Serialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct CertificateInfo {
    pub subject: String,
    pub issuer: String,
    pub serial: String,
    pub not_before: String,
    pub not_after: String,
    pub signature_algorithm: String,
    pub public_key_algorithm: String,
    pub key_size: Option<usize>,
    pub is_ca: bool,
    pub self_signed: bool,
    pub subject_alt_names: Vec<String>,
    pub key_usage: Vec<String>,
}

#[derive(Serialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct CertificatePreview {
    /// `certificate`, `chain`, `private-key`, `public-key` or `unknown`.
    pub kind: String,
    pub certificates: Vec<CertificateInfo>,
}

pub(crate) fn is_certificate_extension(extension: &str) -> bool {
    matches!(
        extension.to_ascii_lowercase().as_str(),
        "pem" | "crt" | "cer" | "der" | "cert"
    )
}

#[tauri::command]
pub async fn open_certificate(
    database: State<'_, Database>,
    clients: State<'_, remote::RemoteClients>,
    sessions: State<'_, network::servers::Sessions>,
    path: String,
) -> Result<CertificatePreview, String> {
    let bytes = read_bytes(&database.0, &clients, &sessions, &path, SOURCE_MAX_BYTES).await?;
    tauri::async_runtime::spawn_blocking(move || parse_certificate(&bytes))
        .await
        .map_err(|error| error.to_string())?
}

fn info_of(cert: &X509Certificate<'_>) -> CertificateInfo {
    let mut info = CertificateInfo {
        subject: cert.subject().to_string(),
        issuer: cert.issuer().to_string(),
        serial: cert.raw_serial_as_string(),
        not_before: cert.validity().not_before.to_string(),
        not_after: cert.validity().not_after.to_string(),
        signature_algorithm: cert.signature_algorithm.algorithm.to_id_string(),
        public_key_algorithm: cert.public_key().algorithm.algorithm.to_id_string(),
        key_size: Some(cert.public_key().raw.len() * 8),
        is_ca: cert.is_ca(),
        self_signed: cert.subject() == cert.issuer(),
        ..Default::default()
    };

    if let Ok(Some(extension)) = cert.subject_alternative_name() {
        for name in &extension.value.general_names {
            let text = match name {
                GeneralName::DNSName(name) => Some((*name).to_string()),
                GeneralName::RFC822Name(name) => Some((*name).to_string()),
                GeneralName::URI(uri) => Some((*uri).to_string()),
                GeneralName::IPAddress(bytes) => Some(
                    bytes
                        .iter()
                        .map(|byte| byte.to_string())
                        .collect::<Vec<_>>()
                        .join("."),
                ),
                _ => None,
            };
            if let Some(text) = text {
                info.subject_alt_names.push(text);
            }
        }
    }

    if let Ok(Some(usage)) = cert.key_usage() {
        let flags = [
            ("Digital Signature", usage.value.digital_signature()),
            ("Non Repudiation", usage.value.non_repudiation()),
            ("Key Encipherment", usage.value.key_encipherment()),
            ("Data Encipherment", usage.value.data_encipherment()),
            ("Key Agreement", usage.value.key_agreement()),
            ("Certificate Sign", usage.value.key_cert_sign()),
            ("CRL Sign", usage.value.crl_sign()),
        ];
        for (label, set) in flags {
            if set {
                info.key_usage.push(label.to_string());
            }
        }
    }

    info
}

pub(crate) fn parse_certificate(bytes: &[u8]) -> Result<CertificatePreview, String> {
    let text = decode_text(bytes).ok();
    if let Some(text) = &text {
        if text.contains("-----BEGIN") {
            return parse_pem(text);
        }
    }

    // DER is a single certificate in binary form.
    match X509Certificate::from_der(bytes) {
        Ok((_, cert)) => Ok(CertificatePreview {
            kind: "certificate".to_string(),
            certificates: vec![info_of(&cert)],
        }),
        Err(_) => Err("Not a certificate or key".to_string()),
    }
}

fn parse_pem(text: &str) -> Result<CertificatePreview, String> {
    let mut certificates = Vec::new();
    let mut saw_key = false;
    let mut saw_public = false;

    let mut rest = text;
    while let Some(start) = rest.find("-----BEGIN ") {
        let after = &rest[start + 11..];
        let Some(label_end) = after.find("-----") else {
            break;
        };
        let label = &after[..label_end];
        let Some(body_start) = rest[start..].find('\n') else {
            break;
        };
        let body = &rest[start + body_start + 1..];
        let Some(end) = body.find("-----END ") else {
            break;
        };
        let encoded: String = body[..end].split_whitespace().collect();
        rest = &body[end..];

        if label.contains("CERTIFICATE") {
            if let Ok(der) = STANDARD.decode(encoded.as_bytes()) {
                if let Ok((_, cert)) = X509Certificate::from_der(&der) {
                    certificates.push(info_of(&cert));
                }
            }
        } else if label.contains("PRIVATE KEY") {
            saw_key = true;
        } else if label.contains("PUBLIC KEY") {
            saw_public = true;
        }
    }

    let kind = if !certificates.is_empty() {
        if certificates.len() > 1 {
            "chain"
        } else {
            "certificate"
        }
    } else if saw_key {
        "private-key"
    } else if saw_public {
        "public-key"
    } else {
        "unknown"
    };

    if kind == "unknown" {
        return Err("No PEM blocks found".to_string());
    }
    Ok(CertificatePreview {
        kind: kind.to_string(),
        certificates,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const CERT: &str = include_str!("../../tests/fixtures/sample.crt");

    #[test]
    fn parses_a_pem_certificate() {
        let preview = parse_certificate(CERT.as_bytes()).unwrap();
        assert_eq!(preview.kind, "certificate");
        assert_eq!(preview.certificates.len(), 1);
        let cert = &preview.certificates[0];
        assert!(cert.subject.contains("lite-explorer"));
        assert!(cert
            .subject_alt_names
            .iter()
            .any(|name| name.contains("lite-explorer")));
    }

    #[test]
    fn flags_private_keys() {
        let preview =
            parse_certificate(b"-----BEGIN PRIVATE KEY-----\nAAAA\n-----END PRIVATE KEY-----\n")
                .unwrap();
        assert_eq!(preview.kind, "private-key");
        assert!(preview.certificates.is_empty());
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_certificate(b"not a certificate").is_err());
    }
}
