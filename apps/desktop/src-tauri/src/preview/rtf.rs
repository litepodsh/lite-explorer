//! RTF preview: converts a Rich Text Format document into sanitized HTML. Only a
//! readable subset is supported — paragraphs, inline styling, colors and Unicode.
//! Destinations (font/color tables, stylesheets, images, info) are skipped.

use std::fs;

use encoding_rs::Encoding;
use serde::Serialize;

use crate::explorer::local_path::{validate_existing, ExpectedKind};

const DEFAULT_SIZE: i32 = 24;
const DEFAULT_UC: usize = 1;

/// Groups whose contents are metadata or binary, never page text.
const DESTINATIONS: &[&str] = &[
    "fonttbl",
    "colortbl",
    "stylesheet",
    "info",
    "generator",
    "listtable",
    "listoverridetable",
    "listoverride",
    "list",
    "listtext",
    "rsidtbl",
    "latentstyles",
    "themedata",
    "colorschememapping",
    "datastore",
    "xmlnstbl",
    "mmathPr",
    "pict",
    "object",
    "header",
    "footer",
    "headerl",
    "headerr",
    "footerl",
    "footerr",
    "footnote",
    "field",
    "fldinst",
    "filetbl",
    "wgrffmtfilter",
    "expandedcolortbl",
    "pnseclvl",
    "pntext",
    "upr",
];

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RtfDocument {
    pub html: String,
}

#[tauri::command]
pub async fn open_rtf(path: String) -> Result<RtfDocument, String> {
    let path = validate_existing(std::path::Path::new(&path), ExpectedKind::File)?;
    tauri::async_runtime::spawn_blocking(move || read_rtf(&path.to_string_lossy()))
        .await
        .map_err(|error| error.to_string())?
}

fn read_rtf(path: &str) -> Result<RtfDocument, String> {
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    to_html(&bytes)
}

/// True when the bytes open an RTF document.
pub(crate) fn looks_like_rtf(bytes: &[u8]) -> bool {
    let start = bytes
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .unwrap_or(bytes.len());
    bytes[start..].starts_with(b"{\\rtf")
}

#[derive(Clone, Copy, PartialEq)]
enum Vert {
    Normal,
    Super,
    Sub,
}

#[derive(Clone)]
struct State {
    bold: bool,
    italic: bool,
    underline: bool,
    strike: bool,
    vert: Vert,
    size: i32,
    color: usize,
    highlight: usize,
    align: &'static str,
}

impl Default for State {
    fn default() -> Self {
        State {
            bold: false,
            italic: false,
            underline: false,
            strike: false,
            vert: Vert::Normal,
            size: DEFAULT_SIZE,
            color: 0,
            highlight: 0,
            align: "",
        }
    }
}

struct Parser<'a> {
    bytes: &'a [u8],
    out: String,
    state: State,
    stack: Vec<State>,
    colors: Vec<Option<String>>,
    codepage: &'static Encoding,
    uc: usize,
    uc_skip: usize,
    high_surrogate: Option<u32>,
    text: Vec<u8>,
    paragraph_open: bool,
}

fn to_html(bytes: &[u8]) -> Result<RtfDocument, String> {
    if !looks_like_rtf(bytes) {
        return Err("Not an RTF document".to_string());
    }
    let mut parser = Parser {
        bytes,
        out: String::new(),
        state: State::default(),
        stack: Vec::new(),
        colors: Vec::new(),
        codepage: encoding_rs::WINDOWS_1252,
        uc: DEFAULT_UC,
        uc_skip: 0,
        high_surrogate: None,
        text: Vec::new(),
        paragraph_open: false,
    };
    parser.run();
    parser.close_paragraph();
    Ok(RtfDocument { html: parser.out })
}

fn encoding_for(codepage: u16) -> &'static Encoding {
    match codepage {
        1250 => encoding_rs::WINDOWS_1250,
        1251 => encoding_rs::WINDOWS_1251,
        1252 => encoding_rs::WINDOWS_1252,
        1253 => encoding_rs::WINDOWS_1253,
        1254 => encoding_rs::WINDOWS_1254,
        1255 => encoding_rs::WINDOWS_1255,
        1256 => encoding_rs::WINDOWS_1256,
        1257 => encoding_rs::WINDOWS_1257,
        1258 => encoding_rs::WINDOWS_1258,
        932 => encoding_rs::SHIFT_JIS,
        936 => encoding_rs::GBK,
        949 => encoding_rs::EUC_KR,
        950 => encoding_rs::BIG5,
        65001 => encoding_rs::UTF_8,
        _ => encoding_rs::WINDOWS_1252,
    }
}

impl Parser<'_> {
    fn run(&mut self) {
        let mut i = 0;
        while i < self.bytes.len() {
            match self.bytes[i] {
                b'{' => {
                    self.flush_text();
                    if let Some(next) = self.group_start(i + 1) {
                        i = next;
                        continue;
                    }
                    self.stack.push(self.state.clone());
                    i += 1;
                }
                b'}' => {
                    self.flush_text();
                    if let Some(previous) = self.stack.pop() {
                        self.state = previous;
                    }
                    i += 1;
                }
                b'\\' => {
                    // Hex escapes are text bytes; keep a run of them in the buffer so
                    // multi-byte code pages decode as one sequence.
                    if self.bytes.get(i + 1) != Some(&b'\'') {
                        self.flush_text();
                    }
                    i = self.control_word(i);
                }
                b'\r' | b'\n' => i += 1,
                _ => {
                    if self.uc_skip > 0 {
                        self.uc_skip -= 1;
                    } else {
                        self.text.push(self.bytes[i]);
                    }
                    i += 1;
                }
            }
        }
        self.flush_text();
    }

    /// If the group opening at `start` is a destination, handles or skips it and
    /// returns the index just past its closing brace.
    fn group_start(&mut self, start: usize) -> Option<usize> {
        if self.bytes.get(start) != Some(&b'\\') {
            return None;
        }
        // `{\*...}` marks an ignorable destination group.
        if self.bytes.get(start + 1) == Some(&b'*') {
            return Some(self.skip_group(start));
        }
        let word = self.peek_word(start + 1);
        if word == "*" {
            return Some(self.skip_group(start));
        }
        if word == "colortbl" || word == "expandedcolortbl" {
            self.parse_colortbl(start);
            return Some(self.skip_group(start));
        }
        if DESTINATIONS.contains(&word.as_str()) {
            return Some(self.skip_group(start));
        }
        None
    }

    fn peek_word(&self, mut i: usize) -> String {
        let mut word = String::new();
        while i < self.bytes.len() && self.bytes[i].is_ascii_alphabetic() {
            word.push(self.bytes[i] as char);
            i += 1;
        }
        word
    }

    /// Skips a group starting at its opening `{` (given `start` inside it).
    fn skip_group(&self, start: usize) -> usize {
        let mut depth = 1usize;
        let mut i = start;
        while i < self.bytes.len() {
            match self.bytes[i] {
                b'\\' => {
                    i += 2;
                    continue;
                }
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        return i + 1;
                    }
                }
                _ => {}
            }
            i += 1;
        }
        self.bytes.len()
    }

    fn parse_colortbl(&mut self, start: usize) {
        let end = self.skip_group(start);
        let slice = &self.bytes[start..end.saturating_sub(1).max(start)];
        let mut i = 0;
        let (mut red, mut green, mut blue) = (None, None, None);
        while i < slice.len() {
            if slice[i] == b'\\' {
                let word = self.peek_word_at(slice, i + 1);
                let mut j = i + 1 + word.len();
                let mut number: i32 = 0;
                let mut has = false;
                while j < slice.len() && slice[j].is_ascii_digit() {
                    number = number * 10 + (slice[j] - b'0') as i32;
                    has = true;
                    j += 1;
                }
                if has {
                    match word.as_str() {
                        "red" => red = Some(number),
                        "green" => green = Some(number),
                        "blue" => blue = Some(number),
                        _ => {}
                    }
                }
                i = j;
            } else if slice[i] == b';' {
                self.colors.push(match (red, green, blue) {
                    (Some(r), Some(g), Some(b)) => Some(format!("rgb({r} {g} {b})")),
                    _ => None,
                });
                red = None;
                green = None;
                blue = None;
                i += 1;
            } else {
                i += 1;
            }
        }
    }

    fn peek_word_at(&self, bytes: &[u8], mut i: usize) -> String {
        let mut word = String::new();
        while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
            word.push(bytes[i] as char);
            i += 1;
        }
        word
    }

    fn control_word(&mut self, backslash: usize) -> usize {
        let mut i = backslash + 1;
        let Some(&byte) = self.bytes.get(i) else {
            return i;
        };
        match byte {
            b'\\' | b'{' | b'}' => {
                self.text.push(byte);
                i + 1
            }
            b'\'' => {
                if let (Some(&hi), Some(&lo)) = (self.bytes.get(i + 1), self.bytes.get(i + 2)) {
                    if self.uc_skip > 0 {
                        self.uc_skip -= 1;
                    } else if let (Some(hi), Some(lo)) = (hex(hi), hex(lo)) {
                        self.text.push(hi * 16 + lo);
                    }
                    return i + 3;
                }
                i + 1
            }
            b'~' => {
                self.text.push(b' ');
                i + 1
            }
            b'-' | b'*' => i + 1,
            b'_' => {
                self.text.push(b'-');
                i + 1
            }
            b'\r' | b'\n' => i + 1,
            _ if byte.is_ascii_alphabetic() => {
                let word = self.peek_word(i);
                i += word.len();
                let mut negative = false;
                if self.bytes.get(i) == Some(&b'-') {
                    negative = true;
                    i += 1;
                }
                let mut number: Option<i32> = None;
                let digits = i;
                while i < self.bytes.len() && self.bytes[i].is_ascii_digit() {
                    i += 1;
                }
                if i > digits {
                    let value: i32 = std::str::from_utf8(&self.bytes[digits..i])
                        .ok()
                        .and_then(|value| value.parse().ok())
                        .unwrap_or(0);
                    number = Some(if negative { -value } else { value });
                }
                if self.bytes.get(i) == Some(&b' ') {
                    i += 1;
                }
                self.apply(&word, number);
                i
            }
            _ => {
                self.text.push(byte);
                i + 1
            }
        }
    }

    fn apply(&mut self, word: &str, number: Option<i32>) {
        let on = number != Some(0);
        match word {
            "b" => self.state.bold = on,
            "i" => self.state.italic = on,
            "ul" => self.state.underline = on,
            "ulnone" => self.state.underline = false,
            "strike" | "striked" => self.state.strike = on,
            "super" => self.state.vert = Vert::Super,
            "sub" => self.state.vert = Vert::Sub,
            "nosupersub" => self.state.vert = Vert::Normal,
            "fs" => {
                if let Some(size) = number {
                    self.state.size = size;
                }
            }
            "cf" => self.state.color = number.unwrap_or(0).max(0) as usize,
            "cb" | "highlight" => self.state.highlight = number.unwrap_or(0).max(0) as usize,
            "qc" => self.state.align = "center",
            "qr" => self.state.align = "right",
            "qj" => self.state.align = "justify",
            "ql" => self.state.align = "",
            "plain" => {
                let align = self.state.align;
                self.state = State {
                    align,
                    ..State::default()
                };
            }
            "pard" => {
                self.state.align = "";
                self.state.size = DEFAULT_SIZE;
            }
            "par" | "sect" | "page" => {
                self.flush_text();
                self.close_paragraph();
            }
            "line" => {
                self.flush_text();
                self.ensure_paragraph();
                self.out.push_str("<br>");
            }
            "tab" => {
                self.flush_text();
                self.ensure_paragraph();
                self.out.push('\t');
            }
            "bullet" => {
                self.flush_text();
                self.push_text("• ");
            }
            "ansicpg" => {
                if let Some(codepage) = number {
                    self.codepage = encoding_for(codepage as u16);
                }
            }
            "uc" => self.uc = number.unwrap_or(0).max(0) as usize,
            "u" | "ud" => {
                if let Some(mut code) = number {
                    if code < 0 {
                        code += 65536;
                    }
                    self.push_codepoint(code as u32);
                    self.uc_skip = self.uc;
                }
            }
            _ => {}
        }
    }

    fn push_codepoint(&mut self, code: u32) {
        match code {
            0xD800..=0xDBFF => self.high_surrogate = Some(code),
            0xDC00..=0xDFFF => {
                if let Some(high) = self.high_surrogate.take() {
                    let combined = 0x10000 + ((high - 0xD800) << 10) + (code - 0xDC00);
                    if let Some(c) = char::from_u32(combined) {
                        let mut buffer = [0u8; 4];
                        self.push_text(c.encode_utf8(&mut buffer));
                    }
                }
            }
            _ => {
                self.high_surrogate = None;
                if let Some(c) = char::from_u32(code) {
                    let mut buffer = [0u8; 4];
                    self.push_text(c.encode_utf8(&mut buffer));
                }
            }
        }
    }

    fn flush_text(&mut self) {
        if self.text.is_empty() {
            return;
        }
        let (decoded, _, _) = self.codepage.decode(&self.text);
        let owned = decoded.into_owned();
        self.text.clear();
        self.push_text(&owned);
    }

    fn ensure_paragraph(&mut self) {
        if self.paragraph_open {
            return;
        }
        self.paragraph_open = true;
        self.out.push_str("<p");
        if !self.state.align.is_empty() {
            self.out
                .push_str(&format!(" style=\"text-align:{}\"", self.state.align));
        }
        self.out.push('>');
    }

    fn close_paragraph(&mut self) {
        if self.paragraph_open {
            self.out.push_str("</p>");
            self.paragraph_open = false;
        }
    }

    fn push_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.ensure_paragraph();
        let mut html = escape(text);
        if self.state.bold {
            html = format!("<strong>{html}</strong>");
        }
        if self.state.italic {
            html = format!("<em>{html}</em>");
        }
        if self.state.underline {
            html = format!("<u>{html}</u>");
        }
        if self.state.strike {
            html = format!("<s>{html}</s>");
        }
        let mut style = String::new();
        if self.state.size != DEFAULT_SIZE {
            style.push_str(&format!("font-size:{}pt;", self.state.size as f64 / 2.0));
        }
        if self.state.color > 0 {
            if let Some(Some(color)) = self.colors.get(self.state.color) {
                style.push_str(&format!("color:{color};"));
            }
        }
        if self.state.highlight > 0 {
            if let Some(Some(color)) = self.colors.get(self.state.highlight) {
                style.push_str(&format!("background-color:{color};"));
            }
        }
        match self.state.vert {
            Vert::Super => style.push_str("vertical-align:super;font-size:smaller;"),
            Vert::Sub => style.push_str("vertical-align:sub;font-size:smaller;"),
            Vert::Normal => {}
        }
        if !style.is_empty() {
            html = format!("<span style=\"{style}\">{html}</span>");
        }
        self.out.push_str(&html);
    }
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn html(rtf: &str) -> String {
        to_html(rtf.as_bytes()).unwrap().html
    }

    #[test]
    fn renders_paragraphs_and_inline_styling() {
        let out = html(
            r"{\rtf1\ansi{\fonttbl{\f0 Arial;}}{\colortbl;\red255\green0\blue0;}
            \f0\fs24 Hola {\b mundo} {\i cruel}\par Adi\'f3s\par}",
        );
        assert!(out.contains("<p>"));
        assert!(out.contains("<strong>mundo</strong>"));
        assert!(out.contains("<em>cruel</em>"));
        assert!(out.contains("Adiós"));
        assert!(!out.contains("fonttbl"));
    }

    #[test]
    fn renders_unicode_and_surrogate_pairs() {
        let out = html(r"{\rtf1\ansi\uc1 A\u241?B\u-10179?\u-8704?C}");
        assert!(out.contains("AñB"), "{out}");
        // U+1F600 (grinning face) encoded as a surrogate pair.
        assert!(out.contains('\u{1F600}'), "{out}");
    }

    #[test]
    fn applies_color_and_size() {
        let out = html(r"{\rtf1\ansi{\colortbl;\red255\green0\blue0;}\fs48\cf1 rojo}");
        assert!(out.contains("font-size:24pt"));
        assert!(out.contains("color:rgb(255 0 0)"));
    }

    #[test]
    fn rejects_non_rtf() {
        assert!(to_html(b"plain text").is_err());
    }
}
