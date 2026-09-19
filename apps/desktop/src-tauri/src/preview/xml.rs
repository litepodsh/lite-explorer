//! Minimal XML tree parser for the preview readers. The documents these readers
//! touch (flat OpenDocument, Office parts) are small and well-formed, so a compact
//! hand-rolled tokenizer is enough and avoids a dependency.

use std::collections::HashMap;

#[derive(Debug, Default, Clone)]
pub(crate) struct Element {
    pub name: String,
    pub attrs: HashMap<String, String>,
    pub children: Vec<Element>,
    /// Raw text; only meaningful on synthetic `#text` nodes, so order is preserved.
    pub text: String,
}

impl Element {
    /// Tag name without its namespace prefix.
    pub fn local(&self) -> &str {
        local_name(&self.name)
    }

    /// Attribute by full or local name, so callers need not know the prefix.
    pub fn attr(&self, name: &str) -> Option<&str> {
        if let Some(value) = self.attrs.get(name) {
            return Some(value);
        }
        let target = local_name(name);
        self.attrs
            .iter()
            .find(|(key, _)| local_name(key) == target)
            .map(|(_, value)| value.as_str())
    }

    pub fn children_named<'a>(&'a self, local: &'a str) -> impl Iterator<Item = &'a Element> {
        self.children
            .iter()
            .filter(move |child| child.local() == local)
    }

    /// First descendant (depth-first, self excluded) with the given local name.
    pub fn descendant(&self, local: &str) -> Option<&Element> {
        for child in &self.children {
            if child.local() == local {
                return Some(child);
            }
            if let Some(found) = child.descendant(local) {
                return Some(found);
            }
        }
        None
    }

    /// All descendants (depth-first, self excluded) with the given local name.
    pub fn descendants_named<'a>(&'a self, local: &'a str) -> impl Iterator<Item = &'a Element> {
        let mut found = Vec::new();
        self.collect_descendants(local, &mut found);
        found.into_iter()
    }

    fn collect_descendants<'a>(&'a self, local: &str, found: &mut Vec<&'a Element>) {
        for child in &self.children {
            if child.local() == local {
                found.push(child);
            }
            child.collect_descendants(local, found);
        }
    }

    /// Visible text: nested paragraphs break lines, `text:s`/`text:tab` expand.
    pub fn text_content(&self) -> String {
        let mut out = String::new();
        self.collect_text(&mut out);
        out
    }

    fn collect_text(&self, out: &mut String) {
        if self.local() == "#text" {
            out.push_str(&self.text);
            return;
        }
        for child in &self.children {
            match child.local() {
                "s" => {
                    let count = child
                        .attr("c")
                        .and_then(|value| value.parse::<usize>().ok())
                        .unwrap_or(1);
                    for _ in 0..count {
                        out.push(' ');
                    }
                }
                "tab" => out.push('\t'),
                "line-break" => out.push('\n'),
                "p" => {
                    if !out.is_empty() && !out.ends_with('\n') {
                        out.push('\n');
                    }
                    child.collect_text(out);
                }
                _ => child.collect_text(out),
            }
        }
    }
}

fn local_name(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

/// Normalizes `..`/`.` segments in a slash-separated path.
pub(crate) fn normalize_path(path: &str) -> String {
    let mut segments: Vec<&str> = Vec::new();
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop();
            }
            segment => segments.push(segment),
        }
    }
    segments.join("/")
}

/// Parses a document into a synthetic `#document` root with the real root as child.
pub(crate) fn parse_document(xml: &str) -> Element {
    let mut root = Element {
        name: "#document".into(),
        ..Element::default()
    };
    let mut stack: Vec<Element> = Vec::new();
    let mut pending_text = String::new();
    let bytes = xml.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] != b'<' {
            let start = i;
            while i < bytes.len() && bytes[i] != b'<' {
                i += 1;
            }
            decode_entities(&xml[start..i], &mut pending_text);
            continue;
        }
        if !pending_text.is_empty() {
            push_text(&mut stack, &mut root, &mut pending_text);
        }
        if xml[i..].starts_with("<!--") {
            i = xml[i..].find("-->").map_or(bytes.len(), |end| i + end + 3);
            continue;
        }
        if xml[i..].starts_with("<![CDATA[") {
            let start = i + 9;
            let end = xml[start..]
                .find("]]>")
                .map_or(bytes.len(), |end| start + end);
            let mut cdata = String::new();
            decode_entities(&xml[start..end], &mut cdata);
            if !cdata.is_empty() {
                push_text(&mut stack, &mut root, &mut cdata);
            }
            i = (end + 3).min(bytes.len());
            continue;
        }
        if xml[i..].starts_with("<?") || xml[i..].starts_with("<!") {
            i = xml[i..].find('>').map_or(bytes.len(), |end| i + end + 1);
            continue;
        }
        let Some(end) = find_tag_end(xml, i) else {
            break;
        };
        let raw = xml[i + 1..end].trim();
        if raw.starts_with('/') {
            if let Some(node) = stack.pop() {
                push_child(&mut stack, &mut root, node);
            }
        } else {
            let self_closing = raw.ends_with('/');
            let raw = raw.trim_end_matches('/');
            let (name, attrs) = parse_tag(raw);
            let element = Element {
                name,
                attrs,
                ..Element::default()
            };
            if self_closing {
                push_child(&mut stack, &mut root, element);
            } else {
                stack.push(element);
            }
        }
        i = end + 1;
    }

    if !pending_text.is_empty() {
        push_text(&mut stack, &mut root, &mut pending_text);
    }
    while let Some(node) = stack.pop() {
        push_child(&mut stack, &mut root, node);
    }
    root
}

/// Text becomes a synthetic `#text` child so it keeps its place among elements.
fn push_text(stack: &mut [Element], root: &mut Element, text: &mut String) {
    let node = Element {
        name: "#text".into(),
        text: std::mem::take(text),
        ..Element::default()
    };
    push_child(stack, root, node);
}

fn push_child(stack: &mut [Element], root: &mut Element, node: Element) {
    if let Some(parent) = stack.last_mut() {
        parent.children.push(node);
    } else {
        root.children.push(node);
    }
}

fn find_tag_end(xml: &str, start: usize) -> Option<usize> {
    let bytes = xml.as_bytes();
    let mut quote: Option<u8> = None;
    let mut i = start + 1;
    while i < bytes.len() {
        let byte = bytes[i];
        match quote {
            Some(q) if byte == q => quote = None,
            Some(_) => {}
            None if byte == b'"' || byte == b'\'' => quote = Some(byte),
            None if byte == b'>' => return Some(i),
            None => {}
        }
        i += 1;
    }
    None
}

fn parse_tag(raw: &str) -> (String, HashMap<String, String>) {
    let name_end = raw.find(char::is_whitespace).unwrap_or(raw.len());
    let name = raw[..name_end].to_string();
    let mut attrs = HashMap::new();
    let rest = &raw[name_end..];
    let bytes = rest.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let key_start = i;
        while i < bytes.len() && bytes[i] != b'=' && !bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let key = &rest[key_start..i];
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if key.is_empty() {
            continue;
        }
        if i < bytes.len() && bytes[i] == b'=' {
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            let value = match bytes.get(i) {
                Some(&quote @ (b'"' | b'\'')) => {
                    i += 1;
                    let value_start = i;
                    while i < bytes.len() && bytes[i] != quote {
                        i += 1;
                    }
                    let value = rest[value_start..i].to_string();
                    if i < bytes.len() {
                        i += 1;
                    }
                    value
                }
                Some(_) => {
                    let value_start = i;
                    while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
                        i += 1;
                    }
                    rest[value_start..i].to_string()
                }
                None => String::new(),
            };
            let mut decoded = String::with_capacity(value.len());
            decode_entities(&value, &mut decoded);
            attrs.insert(key.to_string(), decoded);
        } else {
            attrs.insert(key.to_string(), String::new());
        }
    }
    (name, attrs)
}

/// Decodes the five predefined entities plus numeric character references.
fn decode_entities(text: &str, out: &mut String) {
    if !text.contains('&') {
        out.push_str(text);
        return;
    }
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp..];
        let decoded = rest
            .find(';')
            .filter(|&end| end <= 10)
            .and_then(|end| decode_entity(&rest[1..end]).map(|c| (c, end)));
        match decoded {
            Some((c, end)) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
}

fn decode_entity(entity: &str) -> Option<char> {
    match entity {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        _ => {
            let number = entity.strip_prefix('#')?;
            let code = match number.strip_prefix(['x', 'X']) {
                Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                None => number.parse().ok()?,
            };
            char::from_u32(code)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nested_elements_and_attributes() {
        let root = parse_document(
            r#"<?xml version="1.0"?><a b="1"><c d='2'>hola &amp; adiós</c><c/></a>"#,
        );
        let a = root.children_named("a").next().unwrap();
        assert_eq!(a.attr("b"), Some("1"));
        let first = a.children_named("c").next().unwrap();
        assert_eq!(first.attr("d"), Some("2"));
        assert_eq!(first.text_content(), "hola & adiós");
        assert_eq!(a.children_named("c").count(), 2);
    }

    #[test]
    fn attributes_match_by_local_name() {
        let root = parse_document(r#"<t ns:x="7"/>"#);
        let t = root.children_named("t").next().unwrap();
        assert_eq!(t.attr("x"), Some("7"));
        assert_eq!(t.local(), "t");
    }

    #[test]
    fn text_content_expands_whitespace_elements() {
        let root =
            parse_document(r#"<p>a<text:s text:c="2"/>b<text:tab/>c<text:line-break/>d</p>"#);
        assert_eq!(
            root.children_named("p").next().unwrap().text_content(),
            "a  b\tc\nd"
        );
    }
}
