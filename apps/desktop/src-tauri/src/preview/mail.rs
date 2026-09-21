//! Email preview: `.eml`, Apple Mail `.emlx` and `.mbox` mailboxes, parsed with
//! `mail-parser`. Headers are decoded, the HTML body is returned with inline `cid:`
//! images embedded, and attachments are listed. Read-only; the frontend sanitizes
//! the HTML before rendering.

use std::{fs, io::Cursor};

use base64::{engine::general_purpose::STANDARD, Engine};
use mail_parser::{
    mailbox::mbox::MessageIterator, Address, Message, MessageParser, MimeHeaders, PartType,
};
use serde::Serialize;
use sqlx::SqlitePool;
use tauri::State;

use crate::app::db::Database;
use crate::explorer::local_path::{validate_existing, ExpectedKind};
use crate::search::mail::emlx_message;
use crate::{network, remote};

/// Largest mail file read for preview; bounds huge mailbox dumps.
const MAIL_MAX_BYTES: usize = 32 * 1024 * 1024;
/// Largest inline (cid) image embedded as a data URI.
const INLINE_MAX_BYTES: usize = 2 * 1024 * 1024;
/// Most messages summarized from one `.mbox`.
const MAX_MBOX_MESSAGES: usize = 500;

#[derive(Serialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct MailAttachment {
    pub name: String,
    pub mime: String,
    pub size: usize,
    pub inline: bool,
}

#[derive(Serialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct MailPreview {
    pub subject: String,
    pub from: String,
    pub to: String,
    pub cc: String,
    pub date: Option<String>,
    pub html: Option<String>,
    pub text: Option<String>,
    pub attachments: Vec<MailAttachment>,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct MailSummary {
    pub index: usize,
    pub subject: String,
    pub from: String,
    pub date: Option<String>,
    pub snippet: String,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct MboxPreview {
    pub messages: Vec<MailSummary>,
}

pub(crate) fn is_mail_extension(extension: &str) -> bool {
    matches!(extension.to_ascii_lowercase().as_str(), "eml" | "emlx")
}

pub(crate) fn is_mbox_extension(extension: &str) -> bool {
    matches!(extension.to_ascii_lowercase().as_str(), "mbox" | "mbx")
}

#[tauri::command]
pub async fn open_mail(
    database: State<'_, Database>,
    clients: State<'_, remote::RemoteClients>,
    sessions: State<'_, network::servers::Sessions>,
    path: String,
) -> Result<MailPreview, String> {
    let bytes = read_mail_bytes(&database.0, &clients, &sessions, &path).await?;
    tauri::async_runtime::spawn_blocking(move || parse_message(&path, &bytes))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn open_mbox(
    database: State<'_, Database>,
    clients: State<'_, remote::RemoteClients>,
    sessions: State<'_, network::servers::Sessions>,
    path: String,
) -> Result<MboxPreview, String> {
    let bytes = read_mail_bytes(&database.0, &clients, &sessions, &path).await?;
    tauri::async_runtime::spawn_blocking(move || parse_mbox(&bytes))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn read_mbox_message(
    database: State<'_, Database>,
    clients: State<'_, remote::RemoteClients>,
    sessions: State<'_, network::servers::Sessions>,
    path: String,
    index: usize,
) -> Result<MailPreview, String> {
    let bytes = read_mail_bytes(&database.0, &clients, &sessions, &path).await?;
    tauri::async_runtime::spawn_blocking(move || {
        nth_mbox_message(&bytes, index).ok_or_else(|| "Message not found".to_string())
    })
    .await
    .map_err(|error| error.to_string())?
}

async fn read_mail_bytes(
    pool: &SqlitePool,
    clients: &remote::RemoteClients,
    sessions: &network::servers::Sessions,
    path: &str,
) -> Result<Vec<u8>, String> {
    if network::servers::is_server_path(path) {
        return network::servers::read_bytes(pool, sessions, path, MAIL_MAX_BYTES).await;
    }
    if remote::is_remote_path(path) {
        return remote::read_object(pool, clients, path, MAIL_MAX_BYTES).await;
    }
    read_local_bytes(path)
}

fn read_local_bytes(path: &str) -> Result<Vec<u8>, String> {
    let path = validate_existing(std::path::Path::new(path), ExpectedKind::File)?;
    let metadata = fs::metadata(&path).map_err(|_| "Invalid local path")?;
    if metadata.len() > MAIL_MAX_BYTES as u64 {
        return Err(format!(
            "Mail file is larger than {} MB",
            MAIL_MAX_BYTES / (1024 * 1024)
        ));
    }
    fs::read(path).map_err(|error| error.to_string())
}

fn parse_message(path: &str, bytes: &[u8]) -> Result<MailPreview, String> {
    let raw = if path.to_ascii_lowercase().ends_with(".emlx") {
        emlx_message(bytes)
    } else {
        bytes
    };
    let message = MessageParser::default()
        .parse(raw)
        .ok_or("Unreadable message")?;
    Ok(build_preview(&message))
}

fn parse_mbox(bytes: &[u8]) -> Result<MboxPreview, String> {
    let mut messages = Vec::new();
    for item in MessageIterator::new(Cursor::new(bytes)).map_while(Result::ok) {
        if messages.len() >= MAX_MBOX_MESSAGES {
            break;
        }
        let Some(message) = MessageParser::default().parse(item.contents()) else {
            continue;
        };
        messages.push(MailSummary {
            index: messages.len(),
            subject: message.subject().unwrap_or("(no subject)").to_string(),
            from: message.from().map(addresses).unwrap_or_default(),
            date: message.date().map(|date| date.to_rfc3339()),
            snippet: message
                .body_preview(140)
                .map(|text| text.trim().to_string())
                .unwrap_or_default(),
        });
    }
    if messages.is_empty() {
        return Err("No messages found in mailbox".to_string());
    }
    Ok(MboxPreview { messages })
}

fn nth_mbox_message(bytes: &[u8], index: usize) -> Option<MailPreview> {
    MessageIterator::new(Cursor::new(bytes))
        .map_while(Result::ok)
        .nth(index)
        .and_then(|item| {
            MessageParser::default()
                .parse(item.contents())
                .map(|message| build_preview(&message))
        })
}

fn build_preview(message: &Message<'_>) -> MailPreview {
    let attachments = message.attachments().map(attachment_of).collect::<Vec<_>>();
    let inline = inline_images(message);

    // `body_html` also synthesizes HTML for plain parts; only take a real text/html one.
    let html = (0..message.html_body_count()).find_map(|index| {
        let part = message.html_part(index as u32)?;
        part.is_text_html()
            .then(|| {
                message
                    .body_html(index)
                    .map(|html| embed_inline(&html, &inline))
            })
            .flatten()
    });
    let text = if html.is_none() {
        message.body_text(0).map(|text| text.into_owned())
    } else {
        None
    };

    MailPreview {
        subject: message.subject().unwrap_or("(no subject)").to_string(),
        from: message.from().map(addresses).unwrap_or_default(),
        to: message.to().map(addresses).unwrap_or_default(),
        cc: message.cc().map(addresses).unwrap_or_default(),
        date: message.date().map(|date| date.to_rfc3339()),
        html,
        text,
        attachments,
    }
}

fn attachment_of(part: &mail_parser::MessagePart<'_>) -> MailAttachment {
    let name = part
        .attachment_name()
        .map(str::to_string)
        .or_else(|| {
            part.content_id()
                .map(|id| id.trim_matches(['<', '>']).to_string())
        })
        .unwrap_or_else(|| "attachment".to_string());
    MailAttachment {
        name,
        mime: part
            .content_type()
            .map(mime_of)
            .unwrap_or_else(|| "application/octet-stream".to_string()),
        size: part.body.len(),
        inline: part.content_id().is_some(),
    }
}

fn mime_of(content_type: &mail_parser::ContentType<'_>) -> String {
    match content_type.c_subtype.as_deref() {
        Some(subtype) => format!("{}/{subtype}", content_type.c_type),
        None => content_type.c_type.to_string(),
    }
}

/// `content-id` → `data:` URI for small inline images referenced by `cid:` in HTML.
fn inline_images(message: &Message<'_>) -> Vec<(String, String)> {
    let mut images = Vec::new();
    for part in message.attachments() {
        let (Some(id), Some(content_type)) = (part.content_id(), part.content_type()) else {
            continue;
        };
        if !content_type.c_type.eq_ignore_ascii_case("image") {
            continue;
        }
        let bytes = match &part.body {
            PartType::Binary(bytes) | PartType::InlineBinary(bytes) => bytes,
            _ => continue,
        };
        if bytes.len() > INLINE_MAX_BYTES {
            continue;
        }
        let mime = mime_of(content_type);
        let data = format!("data:{mime};base64,{}", STANDARD.encode(bytes));
        images.push((id.trim_matches(['<', '>']).to_string(), data));
    }
    images
}

fn embed_inline(html: &str, images: &[(String, String)]) -> String {
    let mut out = html.to_string();
    for (id, data) in images {
        out = out.replace(&format!("cid:{id}"), data);
    }
    out
}

fn addresses(address: &Address<'_>) -> String {
    address
        .iter()
        .map(|addr| match (addr.name(), addr.address()) {
            (Some(name), Some(email)) => format!("{name} <{email}>"),
            (Some(name), None) => name.to_string(),
            (None, Some(email)) => email.to_string(),
            (None, None) => String::new(),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAIN: &str = "From: Ana Pérez <ana@example.com>\r\nTo: team@example.com\r\nSubject: =?UTF-8?B?UmV1bmnDs24=?= weekly\r\nDate: Fri, 18 Sep 2026 09:30:00 -0500\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nHola, el informe está listo.";

    #[test]
    fn detects_mail_extensions() {
        assert!(is_mail_extension("EML"));
        assert!(is_mail_extension("emlx"));
        assert!(!is_mail_extension("mbox"));
        assert!(is_mbox_extension("mbox"));
    }

    #[test]
    fn decodes_headers_and_plain_body() {
        let message = MessageParser::default().parse(PLAIN.as_bytes()).unwrap();
        let preview = build_preview(&message);
        assert_eq!(preview.subject, "Reunión weekly");
        assert_eq!(preview.from, "Ana Pérez <ana@example.com>");
        assert_eq!(preview.to, "team@example.com");
        assert!(preview.date.unwrap().starts_with("2026-09-18"));
        assert!(preview.text.unwrap().contains("el informe está listo."));
        assert!(preview.html.is_none());
    }

    #[test]
    fn prefers_html_and_embeds_inline_images() {
        let eml = concat!(
            "From: a@b\r\nSubject: hola\r\n",
            "MIME-Version: 1.0\r\n",
            "Content-Type: multipart/related; boundary=B\r\n\r\n",
            "--B\r\nContent-Type: text/html; charset=utf-8\r\n\r\n",
            "<p>Hola <img src=\"cid:logo@x\"></p>\r\n",
            "--B\r\nContent-Type: image/png\r\nContent-ID: <logo@x>\r\n",
            "Content-Transfer-Encoding: base64\r\n\r\naGVsbG8=\r\n--B--\r\n",
        );
        let message = MessageParser::default().parse(eml.as_bytes()).unwrap();
        let preview = build_preview(&message);
        let html = preview.html.unwrap();
        assert!(html.contains("data:image/png;base64,aGVsbG8="));
        assert!(!html.contains("cid:logo@x"));
        assert_eq!(preview.attachments.len(), 1);
        assert!(preview.attachments[0].inline);
    }

    #[test]
    fn reads_emlx_and_mbox_containers() {
        let emlx = format!("{}\n{PLAIN}<?xml version=\"1.0\"?><plist/>", PLAIN.len());
        let message = MessageParser::default()
            .parse(emlx_message(emlx.as_bytes()))
            .unwrap();
        assert_eq!(build_preview(&message).subject, "Reunión weekly");

        let mbox = format!(
            "From a@b Sat Jan  3 01:05:34 1996\n{PLAIN}\n\nFrom ana@example.com Sat Jan  3 01:05:34 1996\nSubject: second\n\nsecond body\n"
        );
        let messages = MessageIterator::new(Cursor::new(mbox.as_bytes()))
            .map_while(Result::ok)
            .collect::<Vec<_>>();
        assert_eq!(messages.len(), 2);
    }
}
