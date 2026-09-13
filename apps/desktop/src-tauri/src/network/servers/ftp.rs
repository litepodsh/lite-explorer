//! FTP and FTPS (explicit and implicit TLS) with suppaftp. Certificates the system doesn't
//! trust can be pinned by fingerprint (see `known_hosts`).

use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::UNIX_EPOCH,
};

use base64::Engine;
use sha2::{Digest, Sha256};
use suppaftp::tokio_rustls::rustls::{
    self,
    client::{
        danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
        WebPkiServerVerifier,
    },
    crypto::CryptoProvider,
    pki_types::{CertificateDer, ServerName, UnixTime},
    DigitallySignedStruct, RootCertStore, SignatureScheme,
};
use suppaftp::{
    list::{File, ListParser},
    tokio::{AsyncRustlsConnector, AsyncRustlsStream, ImplAsyncFtpStream},
    types::FileType,
    FtpError, Status,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::super::{ConnectError, ErrorKind, Security};
use super::{lost, untrusted, Progress, RemoteEntry, ServerTarget, CHUNK, CONNECT_TIMEOUT};

type Stream = ImplAsyncFtpStream<AsyncRustlsStream>;

pub struct FtpConnection {
    /// FTP runs one command at a time on its control connection.
    stream: tokio::sync::Mutex<Stream>,
}

/// SHA-256 of a certificate, in the same `SHA256:base64` form as SSH fingerprints.
pub fn certificate_fingerprint(der: &[u8]) -> String {
    let digest = Sha256::digest(der);
    format!(
        "SHA256:{}",
        base64::engine::general_purpose::STANDARD_NO_PAD.encode(digest)
    )
}

/// Normal certificate validation, plus a pinned fingerprint for self-signed or private
/// certificates the user trusted.
#[derive(Debug)]
struct PinnedVerifier {
    system: Option<Arc<WebPkiServerVerifier>>,
    provider: Arc<CryptoProvider>,
    known: Option<String>,
    seen: Arc<Mutex<Option<String>>>,
}

impl ServerCertVerifier for PinnedVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let system = match &self.system {
            Some(system) => system.verify_server_cert(
                end_entity,
                intermediates,
                server_name,
                ocsp_response,
                now,
            ),
            None => Err(rustls::Error::InvalidCertificate(
                rustls::CertificateError::UnknownIssuer,
            )),
        };
        if system.is_ok() {
            return system;
        }
        let fingerprint = certificate_fingerprint(end_entity.as_ref());
        *self.seen.lock().unwrap() = Some(fingerprint.clone());
        if self.known.as_deref() == Some(fingerprint.as_str()) {
            Ok(ServerCertVerified::assertion())
        } else {
            system
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

fn tls_connector(
    known: Option<String>,
    seen: Arc<Mutex<Option<String>>>,
) -> Result<AsyncRustlsConnector, ConnectError> {
    let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
    let mut roots = RootCertStore::empty();
    for certificate in rustls_native_certs::load_native_certs().certs {
        let _ = roots.add(certificate);
    }
    let system = WebPkiServerVerifier::builder_with_provider(Arc::new(roots), provider.clone())
        .build()
        .ok();
    let verifier = Arc::new(PinnedVerifier {
        system,
        provider: provider.clone(),
        known,
        seen,
    });
    let config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(ConnectError::other)?
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_no_client_auth();
    Ok(AsyncRustlsConnector::from(
        suppaftp::tokio_rustls::TlsConnector::from(Arc::new(config)),
    ))
}

fn ftp_error(error: FtpError) -> ConnectError {
    match error {
        FtpError::ConnectionError(error)
            if error.kind() == std::io::ErrorKind::ConnectionRefused =>
        {
            ConnectError::new(ErrorKind::Unreachable, "The server refused the connection.")
        }
        FtpError::ConnectionError(_) => lost(),
        FtpError::UnexpectedResponse(response) => {
            let body = String::from_utf8_lossy(&response.body).trim().to_string();
            match response.status {
                Status::NotLoggedIn => ConnectError::new(
                    ErrorKind::Auth,
                    "The server rejected the username or password.",
                ),
                Status::NotAvailable => lost(),
                Status::FileUnavailable => ConnectError::new(
                    ErrorKind::NotFound,
                    if body.is_empty() {
                        "The file or folder isn’t available on the server.".to_string()
                    } else {
                        body
                    },
                ),
                status => ConnectError::other(if body.is_empty() {
                    format!("The server answered {}.", status.code())
                } else {
                    body
                }),
            }
        }
        FtpError::SecureError(message) => {
            ConnectError::other(format!("Secure connection failed: {message}"))
        }
        error => ConnectError::other(error),
    }
}

fn entry(file: &File) -> RemoteEntry {
    RemoteEntry {
        name: file.name().to_string(),
        is_dir: file.is_directory(),
        size: file.size() as u64,
        modified: file
            .modified()
            .duration_since(UNIX_EPOCH)
            .ok()
            .map(|since| since.as_millis() as u64),
    }
}

pub async fn connect(
    target: &ServerTarget,
    username: Option<&str>,
    password: Option<&str>,
    known: Option<String>,
) -> Result<FtpConnection, ConnectError> {
    let seen = Arc::new(Mutex::new(None));
    let security = target.security.unwrap_or(Security::Explicit);
    let connector = match security {
        Security::Plain => None,
        _ => Some(tls_connector(known.clone(), seen.clone())?),
    };
    let address = (target.host.as_str(), target.port);
    let attempt = async {
        match (security, connector) {
            (Security::Implicit, Some(connector)) => {
                Stream::connect_secure_implicit(address, connector, &target.host).await
            }
            (_, Some(connector)) => {
                Stream::connect(address)
                    .await?
                    .into_secure(connector, &target.host)
                    .await
            }
            (_, None) => Stream::connect(address).await,
        }
    };
    let mut stream = match tokio::time::timeout(CONNECT_TIMEOUT, attempt).await {
        Err(_) => {
            return Err(ConnectError::new(
                ErrorKind::Timeout,
                "The server didn’t answer in time.",
            ))
        }
        Ok(Ok(stream)) => stream,
        Ok(Err(error)) => {
            let presented = seen.lock().unwrap().take();
            if let (FtpError::SecureError(_), Some(fingerprint)) = (&error, presented) {
                if known.as_deref() != Some(fingerprint.as_str()) {
                    return Err(untrusted(
                        target,
                        "certificate".into(),
                        fingerprint,
                        known.is_some(),
                    ));
                }
            }
            return Err(ftp_error(error));
        }
    };
    stream
        .login(
            username.unwrap_or("anonymous"),
            password.unwrap_or("anonymous@"),
        )
        .await
        .map_err(ftp_error)?;
    stream
        .transfer_type(FileType::Binary)
        .await
        .map_err(ftp_error)?;
    Ok(FtpConnection {
        stream: tokio::sync::Mutex::new(stream),
    })
}

impl FtpConnection {
    pub async fn home(&self) -> Result<String, ConnectError> {
        self.stream.lock().await.pwd().await.map_err(ftp_error)
    }

    pub async fn list(&self, dir: &str) -> Result<Vec<RemoteEntry>, ConnectError> {
        let mut stream = self.stream.lock().await;
        let files: Vec<File> = match stream.mlsd(Some(dir)).await {
            Ok(lines) => lines
                .iter()
                .filter_map(|line| ListParser::parse_mlsd(line).ok())
                .collect(),
            // Servers without MLSD answer 500/502; fall back to LIST.
            Err(FtpError::UnexpectedResponse(_)) => stream
                .list(Some(dir))
                .await
                .map_err(ftp_error)?
                .iter()
                .filter_map(|line| File::try_from(line.as_str()).ok())
                .collect(),
            Err(error) => return Err(ftp_error(error)),
        };
        Ok(files
            .iter()
            .filter(|file| !matches!(file.name(), "." | "..") && !file.name().contains('/'))
            .map(entry)
            .collect())
    }

    pub async fn stat(&self, path: &str) -> Result<RemoteEntry, ConnectError> {
        let name = super::file_name(path);
        if name.is_empty() {
            return Ok(RemoteEntry {
                name: String::new(),
                is_dir: true,
                size: 0,
                modified: None,
            });
        }
        self.list(super::parent(path))
            .await?
            .into_iter()
            .find(|entry| entry.name == name)
            .ok_or_else(|| {
                ConnectError::new(
                    ErrorKind::NotFound,
                    "The file or folder doesn’t exist on the server.",
                )
            })
    }

    pub async fn mkdir(&self, path: &str) -> Result<(), ConnectError> {
        self.stream
            .lock()
            .await
            .mkdir(path)
            .await
            .map_err(ftp_error)
    }

    pub async fn create_file(&self, path: &str) -> Result<(), ConnectError> {
        let mut empty: &[u8] = &[];
        self.stream
            .lock()
            .await
            .put_file(path, &mut empty)
            .await
            .map(|_| ())
            .map_err(ftp_error)
    }

    pub async fn rename(&self, from: &str, to: &str) -> Result<(), ConnectError> {
        self.stream
            .lock()
            .await
            .rename(from, to)
            .await
            .map_err(ftp_error)
    }

    pub async fn remove_file(&self, path: &str) -> Result<(), ConnectError> {
        self.stream.lock().await.rm(path).await.map_err(ftp_error)
    }

    pub async fn remove_dir(&self, path: &str) -> Result<(), ConnectError> {
        self.stream
            .lock()
            .await
            .rmdir(path)
            .await
            .map_err(ftp_error)
    }

    pub async fn read_head(&self, path: &str, max: usize) -> Result<Vec<u8>, ConnectError> {
        let mut stream = self.stream.lock().await;
        let mut transfer = stream.retr_as_stream(path).await.map_err(ftp_error)?;
        let mut bytes = Vec::new();
        let mut buffer = vec![0u8; CHUNK];
        while bytes.len() < max {
            let read = transfer.read(&mut buffer).await.map_err(|_| lost())?;
            if read == 0 {
                break;
            }
            bytes.extend_from_slice(&buffer[..read]);
        }
        if bytes.len() < max {
            transfer.finish().await.map_err(ftp_error)?;
        } else {
            // Stop early: dropping closes the data socket and the next command reads the reply.
            drop(transfer);
            bytes.truncate(max);
        }
        Ok(bytes)
    }

    pub async fn download(
        &self,
        path: &str,
        local: &Path,
        progress: &mut Progress,
    ) -> Result<(), ConnectError> {
        let mut stream = self.stream.lock().await;
        let mut transfer = stream.retr_as_stream(path).await.map_err(ftp_error)?;
        let mut output = tokio::fs::File::create(local)
            .await
            .map_err(ConnectError::other)?;
        let mut buffer = vec![0u8; CHUNK];
        loop {
            if progress.cancelled() {
                return Err(ConnectError::other("Canceled."));
            }
            let read = transfer.read(&mut buffer).await.map_err(|_| lost())?;
            if read == 0 {
                break;
            }
            output
                .write_all(&buffer[..read])
                .await
                .map_err(ConnectError::other)?;
            progress.add_bytes(read as u64);
        }
        output.flush().await.map_err(ConnectError::other)?;
        transfer.finish().await.map_err(ftp_error)
    }

    pub async fn upload(
        &self,
        local: &Path,
        path: &str,
        progress: &mut Progress,
    ) -> Result<(), ConnectError> {
        let mut input = tokio::fs::File::open(local)
            .await
            .map_err(ConnectError::other)?;
        let mut stream = self.stream.lock().await;
        let mut transfer = stream.put_with_stream(path).await.map_err(ftp_error)?;
        let mut buffer = vec![0u8; CHUNK];
        loop {
            if progress.cancelled() {
                return Err(ConnectError::other("Canceled."));
            }
            let read = input.read(&mut buffer).await.map_err(ConnectError::other)?;
            if read == 0 {
                break;
            }
            transfer
                .write_all(&buffer[..read])
                .await
                .map_err(|_| lost())?;
            progress.add_bytes(read as u64);
        }
        transfer.flush().await.map_err(|_| lost())?;
        transfer.finish().await.map_err(ftp_error)
    }

    pub async fn close(&self) {
        let _ = self.stream.lock().await.quit().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprints_use_the_ssh_format() {
        assert_eq!(
            certificate_fingerprint(b"hello"),
            "SHA256:LPJNul+wow4m6DsqxbninhsWHlwfp0JecwQzYpOLmCQ"
        );
    }
}
