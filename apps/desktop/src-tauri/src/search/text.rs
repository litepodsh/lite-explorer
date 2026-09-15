//! Byte-to-text decoding for content search: UTF-8, UTF-16 (with or without BOM) and
//! Windows-1252 fallback, plus RTF markup stripping.

use std::borrow::Cow;

use encoding_rs::{Encoding, UTF_16BE, UTF_16LE, WINDOWS_1252};

/// Decodes file bytes as text, or `None` for binary content.
pub(super) fn decode(bytes: &[u8]) -> Option<Cow<'_, str>> {
    if let Some((encoding, bom)) = Encoding::for_bom(bytes) {
        let (text, _) = encoding.decode_without_bom_handling(&bytes[bom..]);
        return Some(text);
    }
    if let Some(encoding) = utf16_without_bom(bytes) {
        return Some(encoding.decode_without_bom_handling(bytes).0);
    }
    if bytes.contains(&0) {
        return None;
    }
    match std::str::from_utf8(bytes) {
        Ok(text) => Some(Cow::Borrowed(text)),
        // Legacy text (e.g. CSV exported by Excel on Windows) is usually Windows-1252.
        Err(_) => Some(WINDOWS_1252.decode_without_bom_handling(bytes).0),
    }
}

/// Detects BOM-less UTF-16 from the NUL pattern of mostly-ASCII text: every other byte is zero.
fn utf16_without_bom(bytes: &[u8]) -> Option<&'static Encoding> {
    let sample = &bytes[..bytes.len().min(4096) & !1];
    let pairs = sample.len() / 2;
    if pairs < 2 {
        return None;
    }
    let even_zeros = sample.iter().step_by(2).filter(|&&b| b == 0).count();
    let odd_zeros = sample.iter().skip(1).step_by(2).filter(|&&b| b == 0).count();
    if odd_zeros * 10 >= pairs * 4 && even_zeros * 20 < pairs {
        Some(UTF_16LE)
    } else if even_zeros * 10 >= pairs * 4 && odd_zeros * 20 < pairs {
        Some(UTF_16BE)
    } else {
        None
    }
}

/// Groups whose content is formatting data rather than document text.
const RTF_SKIPPED_DESTINATIONS: &[&str] = &[
    "fonttbl", "colortbl", "stylesheet", "info", "pict", "object", "themedata", "colorschememapping",
    "latentstyles", "datastore", "xmlnstbl", "listtable", "listoverridetable", "rsidtbl", "generator",
    "filetbl", "revtbl", "pgdsctbl", "fldinst",
];

#[derive(Clone, Copy)]
struct RtfGroup {
    skip: bool,
    /// Characters that stand in for each `\u` escape, from `\ucN`.
    fallback_chars: usize,
}

/// Collects RTF text. Escaped `\'hh` bytes are buffered so multi-byte code pages
/// (e.g. Shift-JIS from `\ansicpg932`) decode as whole characters.
struct RtfWriter {
    out: String,
    bytes: Vec<u8>,
    encoding: &'static Encoding,
    /// Fallback characters still to drop after a `\u` escape.
    pending_skip: usize,
}

impl RtfWriter {
    fn byte(&mut self, group: &RtfGroup, byte: u8) {
        if self.pending_skip > 0 {
            self.pending_skip -= 1;
        } else if !group.skip {
            self.bytes.push(byte);
        }
    }

    fn char(&mut self, group: &RtfGroup, c: char) {
        if self.pending_skip > 0 {
            self.pending_skip -= 1;
        } else if !group.skip {
            self.flush();
            self.out.push(c);
        }
    }

    fn flush(&mut self) {
        if !self.bytes.is_empty() {
            self.out.push_str(&self.encoding.decode_without_bom_handling(&self.bytes).0);
            self.bytes.clear();
        }
    }
}

/// Extracts the visible text of an RTF document, one paragraph per line.
pub(super) fn rtf_text(rtf: &[u8]) -> String {
    let mut writer = RtfWriter { out: String::new(), bytes: Vec::new(), encoding: WINDOWS_1252, pending_skip: 0 };
    let mut group = RtfGroup { skip: false, fallback_chars: 1 };
    let mut stack = Vec::new();
    let mut i = 0;
    while i < rtf.len() {
        let byte = rtf[i];
        i += 1;
        match byte {
            b'{' => {
                stack.push(group);
                writer.pending_skip = 0;
            }
            b'}' => {
                group = stack.pop().unwrap_or(group);
                writer.pending_skip = 0;
            }
            b'\r' | b'\n' => {}
            b'\\' => {
                let Some(&next) = rtf.get(i) else { break };
                match next {
                    b'\\' | b'{' | b'}' => {
                        i += 1;
                        writer.byte(&group, next);
                    }
                    b'\'' => {
                        let hex = rtf.get(i + 1..i + 3).and_then(|hex| std::str::from_utf8(hex).ok());
                        i += 3;
                        if let Some(code) = hex.and_then(|hex| u8::from_str_radix(hex, 16).ok()) {
                            writer.byte(&group, code);
                        }
                    }
                    b'*' => {
                        i += 1;
                        group.skip = true;
                    }
                    b'~' => {
                        i += 1;
                        writer.char(&group, ' ');
                    }
                    b'\r' | b'\n' => {
                        i += 1;
                        writer.char(&group, '\n');
                    }
                    b'a'..=b'z' | b'A'..=b'Z' => {
                        let start = i;
                        while rtf.get(i).is_some_and(u8::is_ascii_alphabetic) {
                            i += 1;
                        }
                        let word = std::str::from_utf8(&rtf[start..i]).unwrap_or("");
                        let param_start = i;
                        if rtf.get(i) == Some(&b'-') {
                            i += 1;
                        }
                        while rtf.get(i).is_some_and(u8::is_ascii_digit) {
                            i += 1;
                        }
                        let param: Option<i32> = std::str::from_utf8(&rtf[param_start..i]).ok().and_then(|p| p.parse().ok());
                        if rtf.get(i) == Some(&b' ') {
                            i += 1;
                        }
                        match word {
                            "par" | "line" | "row" | "sect" | "page" => writer.char(&group, '\n'),
                            "tab" | "cell" => writer.char(&group, '\t'),
                            "uc" => group.fallback_chars = param.unwrap_or(1).max(0) as usize,
                            "u" => {
                                let code = param.unwrap_or(0);
                                let code = if code < 0 { code + 65_536 } else { code } as u32;
                                writer.char(&group, char::from_u32(code).unwrap_or('\u{FFFD}'));
                                writer.pending_skip = group.fallback_chars;
                            }
                            "ansicpg" => {
                                writer.flush();
                                if let Some(encoding) = param.and_then(|cp| u16::try_from(cp).ok()).and_then(codepage::to_encoding) {
                                    writer.encoding = encoding;
                                }
                            }
                            _ if RTF_SKIPPED_DESTINATIONS.contains(&word) => group.skip = true,
                            _ => {}
                        }
                    }
                    _ => i += 1,
                }
            }
            _ => writer.byte(&group, byte),
        }
    }
    writer.flush();
    writer.out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utf16le(text: &str, bom: bool) -> Vec<u8> {
        let mut bytes = if bom { vec![0xFF, 0xFE] } else { Vec::new() };
        bytes.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
        bytes
    }

    #[test]
    fn decodes_utf16_with_and_without_bom() {
        assert_eq!(decode(&utf16le("id,name\r\n1,Zarpa", true)).as_deref(), Some("id,name\r\n1,Zarpa"));
        assert_eq!(decode(&utf16le("id,name\r\n1,Zarpa", false)).as_deref(), Some("id,name\r\n1,Zarpa"));
        let be: Vec<u8> = "hello world".encode_utf16().flat_map(u16::to_be_bytes).collect();
        assert_eq!(decode(&be).as_deref(), Some("hello world"));
    }

    #[test]
    fn falls_back_to_windows_1252_and_rejects_binary() {
        assert_eq!(decode(b"caf\xe9").as_deref(), Some("café"));
        assert_eq!(decode("café".as_bytes()).as_deref(), Some("café"));
        assert_eq!(decode(b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR"), None);
    }

    #[test]
    fn strips_rtf_markup() {
        let rtf = br"{\rtf1\ansi\deff0{\fonttbl{\f0 Arial;}}{\colortbl;\red0\green0\blue0;}{\*\generator Word;}
\f0\fs24 Hello {\b Zarpa} caf\'e9\par
Price:\tab 10 \{EUR\}\par
\uc1\u8364? total\par}";
        let text = rtf_text(rtf);
        assert_eq!(text, "Hello Zarpa café\nPrice:\t10 {EUR}\n€ total\n");
    }

    #[test]
    fn decodes_rtf_code_page_bytes() {
        // テスト in Shift-JIS, as Word writes it with \ansicpg932.
        let rtf = br"{\rtf1\ansi\ansicpg932 {\'83\'65\'83\'58\'83\'67} ok\par}";
        assert_eq!(rtf_text(rtf), "テスト ok\n");
    }
}
