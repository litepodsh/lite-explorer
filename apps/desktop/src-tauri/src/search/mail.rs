//! Email content search: `.eml`, Apple Mail `.emlx` and `.mbox` mailboxes. Headers and
//! bodies are decoded (base64, quoted-printable, charsets, HTML) before matching.

use std::io::BufRead;

use mail_parser::{mailbox::mbox::MessageIterator, Address, MessageParser, PartType};

use super::{documents::xml_text, matching_line};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum MailKind {
    Eml,
    Emlx,
    Mbox,
}

pub(super) fn kind_for(extension: &str) -> Option<MailKind> {
    match extension {
        "eml" => Some(MailKind::Eml),
        "emlx" => Some(MailKind::Emlx),
        "mbox" | "mbx" => Some(MailKind::Mbox),
        _ => None,
    }
}

/// First matching line of a mail file held in memory.
pub(super) fn mail_match(bytes: &[u8], kind: MailKind, needle: &str) -> Option<String> {
    match kind {
        MailKind::Eml => message_match(bytes, needle),
        MailKind::Emlx => message_match(emlx_message(bytes), needle),
        MailKind::Mbox => mbox_match(bytes, needle),
    }
}

/// First matching line across the messages of an mbox stream.
pub(super) fn mbox_match(reader: impl BufRead, needle: &str) -> Option<String> {
    MessageIterator::new(reader).map_while(Result::ok).find_map(|message| message_match(message.contents(), needle))
}

/// An `.emlx` file is the message byte count on the first line, the message, then a plist.
fn emlx_message(bytes: &[u8]) -> &[u8] {
    let Some(newline) = bytes.iter().position(|&b| b == b'\n') else { return bytes };
    let body = &bytes[newline + 1..];
    match std::str::from_utf8(&bytes[..newline]).ok().and_then(|count| count.trim().parse::<usize>().ok()) {
        Some(count) => &body[..count.min(body.len())],
        None => bytes,
    }
}

fn message_match(bytes: &[u8], needle: &str) -> Option<String> {
    let message = MessageParser::default().parse(bytes)?;
    let headers = [("Subject", message.subject().map(str::to_string)), ("From", message.from().map(addresses)), ("To", message.to().map(addresses))];
    for (label, value) in headers {
        if let Some(line) = value.and_then(|value| matching_line(&value, needle)) {
            return Some(format!("{label}: {line}"));
        }
    }
    (0..message.text_body_count()).find_map(|index| {
        let part = message.text_part(index as u32)?;
        match &part.body {
            PartType::Text(text) => matching_line(text, needle),
            // Our markup stripper keeps block elements on separate lines, which reads better in snippets.
            PartType::Html(html) => matching_line(&xml_text(html), needle),
            _ => None,
        }
    })
}

fn addresses(address: &Address) -> String {
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

    const MESSAGE: &str = "From: Ana Pérez <ana@example.com>\r\nTo: team@example.com\r\nSubject: =?UTF-8?B?UmV1bmnDs24=?= weekly\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Transfer-Encoding: quoted-printable\r\n\r\nHola,\r\nel informe de Zarpa est=C3=A1 listo.\r\n";

    #[test]
    fn matches_decoded_headers_and_bodies() {
        assert_eq!(mail_match(MESSAGE.as_bytes(), MailKind::Eml, "reunión").as_deref(), Some("Subject: Reunión weekly"));
        assert_eq!(mail_match(MESSAGE.as_bytes(), MailKind::Eml, "ana@").as_deref(), Some("From: Ana Pérez <ana@example.com>"));
        assert_eq!(mail_match(MESSAGE.as_bytes(), MailKind::Eml, "está listo").as_deref(), Some("el informe de Zarpa está listo."));
        assert_eq!(mail_match(MESSAGE.as_bytes(), MailKind::Eml, "missing"), None);

        let html = "Subject: x\r\nContent-Type: text/html; charset=utf-8\r\n\r\n<ul><li>Webinar</li><li>Hackatón Zarpa</li></ul>";
        assert_eq!(mail_match(html.as_bytes(), MailKind::Eml, "zarpa").as_deref(), Some("Hackatón Zarpa"));
    }

    #[test]
    fn reads_emlx_and_mbox_containers() {
        let emlx = format!("{}\n{MESSAGE}<?xml version=\"1.0\"?><plist>zarpa-plist</plist>", MESSAGE.len());
        assert_eq!(mail_match(emlx.as_bytes(), MailKind::Emlx, "zarpa").as_deref(), Some("el informe de Zarpa está listo."));
        assert_eq!(mail_match(emlx.as_bytes(), MailKind::Emlx, "zarpa-plist"), None);

        let mbox = format!("From a@b Sat Jan  3 01:05:34 1996\nSubject: first\n\nnothing here\n\nFrom ana@example.com Sat Jan  3 01:05:34 1996\n{}", MESSAGE.replace("\r\n", "\n"));
        assert_eq!(mail_match(mbox.as_bytes(), MailKind::Mbox, "zarpa").as_deref(), Some("el informe de Zarpa está listo."));
    }
}
