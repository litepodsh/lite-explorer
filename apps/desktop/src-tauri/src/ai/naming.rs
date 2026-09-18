//! Prompt building and answer clean-up for Apple Intelligence name suggestions.
//!
//! Everything here is pure: no model, no IPC, no filesystem policy beyond reading the item. That
//! keeps the interesting behaviour (what the model is told, and what we accept back) unit-tested.

use std::fs;
use std::io::Read;
use std::path::Path;

/// How many sibling names to show the model so it can copy the folder's naming style.
const SIBLING_LIMIT: usize = 24;
/// Characters of file content shown to the model. The on-device context window is 4096 tokens on
/// macOS 26, so the excerpt stays small enough to leave room for the answer.
const EXCERPT_CHARS: usize = 1200;
/// Bytes read to build the excerpt before it is trimmed to `EXCERPT_CHARS`.
const EXCERPT_BYTES: u64 = 64 * 1024;
/// Longest name accepted from the model. Anything longer is the model rambling, not a name.
const MAX_NAME_CHARS: usize = 120;
/// Longest sibling name put in the prompt.
const MAX_SIBLING_CHARS: usize = 48;

/// What the model is told about the item being renamed.
pub struct NameContext {
    pub name: String,
    /// Name of the containing folder, used as a hint about the item's subject.
    pub folder: String,
    pub is_folder: bool,
    pub siblings: Vec<String>,
    pub excerpt: Option<String>,
    /// A picture: the model gets no look at its contents, only its name and folder.
    pub is_picture: bool,
    /// The extension the answer has to keep, taken from the current name.
    pub extension: Option<String>,
}

/// Extensions the on-device model cannot help itself with: it reads no images, so a picture is
/// named from its name, its folder and its neighbours — and told so, to keep it from inventing
/// what it cannot see.
const PICTURE_EXTENSIONS: [&str; 8] = [
    "png", "jpg", "jpeg", "heic", "gif", "webp", "tiff", "avif",
];

pub fn is_picture(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            PICTURE_EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str())
        })
}

/// Reads the item's metadata, its folder's other names, and (for text files) a content excerpt.
pub fn context_for(path: &Path) -> Result<NameContext, String> {
    let metadata = fs::metadata(path).map_err(|error| error.to_string())?;
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    if name.is_empty() {
        return Err("this item has no name".into());
    }
    let is_folder = metadata.is_dir();
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_owned());
    let folder = path
        .parent()
        .and_then(|parent| parent.file_name())
        .map(|folder| folder.to_string_lossy().into_owned())
        .unwrap_or_default();
    let siblings = sibling_names(path.parent(), &name);
    let excerpt = if is_folder { None } else { read_excerpt(path) };
    Ok(NameContext {
        name,
        folder,
        is_folder,
        siblings,
        excerpt,
        is_picture: !is_folder && is_picture(path),
        extension,
    })
}

/// Other names in the same folder: the strongest signal for the naming style to match.
fn sibling_names(parent: Option<&Path>, excluded: &str) -> Vec<String> {
    let Some(parent) = parent else {
        return Vec::new();
    };
    let Ok(entries) = fs::read_dir(parent) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name != excluded && !name.starts_with('.'))
        .map(|name| truncate_chars(&name, MAX_SIBLING_CHARS))
        .collect();
    names.sort();
    names.truncate(SIBLING_LIMIT);
    names
}

/// First slice of a text file, or `None` for anything binary (a binary excerpt is noise).
fn read_excerpt(path: &Path) -> Option<String> {
    let file = fs::File::open(path).ok()?;
    let mut bytes = Vec::new();
    file.take(EXCERPT_BYTES).read_to_end(&mut bytes).ok()?;
    if bytes.contains(&0) {
        return None;
    }
    let text = String::from_utf8_lossy(&bytes);
    let excerpt: String = text.chars().take(EXCERPT_CHARS).collect();
    let excerpt = excerpt.trim();
    if excerpt.is_empty() {
        return None;
    }
    Some(excerpt.to_owned())
}

/// The rules the answer has to follow.
pub fn system_prompt(is_folder: bool, extension: Option<&str>) -> String {
    let kind = if is_folder { "folder" } else { "file" };
    let extension_rule = match extension {
        Some(extension) => format!("- The name must end with `.{extension}`.\n"),
        None => "- Keep the file extension the name already has.\n".to_string(),
    };
    format!(
        "You rename {kind}s for a macOS file manager. Reply with one {kind} name and nothing else.\n\
         Rules:\n\
         {extension_rule}\
         - Never include a path, a slash, a colon, or quotes.\n\
         - Stay under 8 words.\n\
         - Copy the naming style of the other items in the same folder when they agree on one.\n\
         - Use the folder name as a hint about the subject.\n\
         - Answer in the same language as the current name.\n\
         - Make the name clearer and more descriptive, not longer for its own sake."
    )
}

/// The item itself: name, location, neighbours, and content when it is readable text.
pub fn user_prompt(context: &NameContext) -> String {
    let mut prompt = String::new();
    prompt.push_str(&format!("Current name: {}\n", context.name));
    if !context.folder.is_empty() {
        prompt.push_str(&format!("Inside folder: {}\n", context.folder));
    }
    prompt.push_str(&format!(
        "Kind: {}\n",
        if context.is_folder { "folder" } else { "file" }
    ));
    if !context.siblings.is_empty() {
        prompt.push_str(&format!(
            "Other items in this folder: {}\n",
            context.siblings.join(", ")
        ));
    }
    if context.is_picture {
        prompt.push_str(
            "This is a picture. You cannot see it: name it from its name, its folder and its neighbours.\n",
        );
    }
    if let Some(excerpt) = &context.excerpt {
        prompt.push_str(&format!("File contents (excerpt):\n---\n{excerpt}\n---\n"));
    }
    prompt.push_str("Suggest a better name.");
    prompt
}

/// Guided-generation schema: one short string instead of free text the caller has to parse.
pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "name": { "type": "string", "description": "The new file or folder name." }
        },
        "required": ["name"],
        "additionalProperties": false
    })
}

/// Turns a model answer into a name `rename_item` will accept: one line, no path separators, no
/// control characters, short enough to be a name, and still carrying the extension it came with.
pub fn sanitize_name(raw: &str, original_extension: Option<&str>) -> Option<String> {
    let first_line = raw.lines().next().unwrap_or("").trim();
    let unquoted = first_line.trim_matches(|char| matches!(char, '"' | '\'' | '`' | '*'));
    let mut cleaned = String::with_capacity(unquoted.len());
    for char in unquoted.chars() {
        match char {
            // '/' and ':' would escape the folder; the rest are noise in a name.
            '/' | '\\' | ':' | '|' => cleaned.push('-'),
            // A tab or newline is a separator, not junk: keep it as a space so the words below
            // stay apart instead of running together.
            char if char.is_whitespace() => cleaned.push(' '),
            char if char.is_control() => {}
            char => cleaned.push(char),
        }
    }
    let name = collapse_and_trim(&cleaned);
    if name.is_empty() || name == "." || name == ".." {
        return None;
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return None;
    }
    Some(with_extension(&name, original_extension))
}

/// Parses a `kern.osproductversion` / `sw_vers` version string into its major and minor parts.
pub fn parse_os_version(text: &str) -> Option<(i32, i32)> {
    let mut parts = text.trim().split('.');
    let major: i32 = parts.next()?.parse().ok()?;
    let minor: i32 = parts.next().and_then(|part| part.parse().ok()).unwrap_or(0);
    Some((major, minor))
}

fn collapse_and_trim(text: &str) -> String {
    let collapsed: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed.trim().trim_end_matches('.').trim().to_owned()
}

/// Puts the original extension back on: the model is asked to keep it, but renaming must never
/// change a file's type, so it is enforced here rather than trusted.
fn with_extension(name: &str, extension: Option<&str>) -> String {
    let Some(extension) = extension else {
        return name.to_owned();
    };
    let stem = name.trim_end_matches('.');
    if stem
        .rsplit('.')
        .next()
        .is_some_and(|tail| tail.eq_ignore_ascii_case(extension) && stem.len() > extension.len() + 1)
    {
        return name.to_owned();
    }
    // A trailing dot would leave the name ending in ".."; drop it before appending.
    format!("{}.{}", stem.trim_end_matches('.'), extension)
}

fn truncate_chars(text: &str, limit: usize) -> String {
    text.chars().take(limit).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_a_plain_name() {
        assert_eq!(
            sanitize_name("Quarterly Report.pdf", Some("pdf")).as_deref(),
            Some("Quarterly Report.pdf")
        );
    }

    #[test]
    fn restores_the_original_extension() {
        assert_eq!(
            sanitize_name("Neon Terminal", Some("jpg")).as_deref(),
            Some("Neon Terminal.jpg")
        );
        assert_eq!(sanitize_name("notes", None).as_deref(), Some("notes"));
    }

    #[test]
    fn pictures_are_named_without_being_seen() {
        assert!(is_picture(Path::new("/a/b/photo.JPG")));
        assert!(is_picture(Path::new("/a/b/shot.heic")));
        assert!(!is_picture(Path::new("/a/b/notes.md")));
        assert!(!is_picture(Path::new("/a/b/archive.zip")));
    }

    #[test]
    fn uses_only_the_first_line() {
        assert_eq!(
            sanitize_name("Notes.md\nSure! Here you go.", Some("md")).as_deref(),
            Some("Notes.md")
        );
    }

    #[test]
    fn strips_wrapping_quotes_and_marks() {
        assert_eq!(
            sanitize_name("\"Budget 2026.xlsx\"", Some("xlsx")).as_deref(),
            Some("Budget 2026.xlsx")
        );
        assert_eq!(sanitize_name("`main.rs`", Some("rs")).as_deref(), Some("main.rs"));
    }

    #[test]
    fn turns_path_separators_into_dashes() {
        assert_eq!(
            sanitize_name("reports/2026/summary.pdf", Some("pdf")).as_deref(),
            Some("reports-2026-summary.pdf")
        );
        assert_eq!(sanitize_name("notes:final", None).as_deref(), Some("notes-final"));
    }

    #[test]
    fn collapses_whitespace_and_control_characters() {
        assert_eq!(
            sanitize_name("  Trip   photos\t2026 ", None).as_deref(),
            Some("Trip photos 2026")
        );
        assert_eq!(sanitize_name("na\u{7}me", None).as_deref(), Some("name"));
    }

    #[test]
    fn rejects_names_that_cannot_be_used() {
        assert_eq!(sanitize_name("   ", None), None);
        assert_eq!(sanitize_name("", None), None);
        assert_eq!(sanitize_name("..", None), None);
        assert_eq!(sanitize_name(&"a".repeat(MAX_NAME_CHARS + 1), None), None);
    }

    #[test]
    fn reads_os_versions() {
        assert_eq!(parse_os_version("26.1\n"), Some((26, 1)));
        assert_eq!(parse_os_version("26"), Some((26, 0)));
        assert_eq!(parse_os_version("15.6.1"), Some((15, 6)));
        assert_eq!(parse_os_version("nonsense"), None);
    }

    #[test]
    fn prompt_mentions_extension_rules_and_neighbours() {
        let context = NameContext {
            name: "IMG_4021.HEIC".into(),
            folder: "Barcelona".into(),
            is_folder: false,
            siblings: vec!["barcelona-beach.heic".into(), "barcelona-food.heic".into()],
            excerpt: None,
            is_picture: true,
            extension: Some("HEIC".into()),
        };
        let prompt = user_prompt(&context);
        assert!(prompt.contains("IMG_4021.HEIC"));
        assert!(prompt.contains("Barcelona"));
        assert!(prompt.contains("barcelona-beach.heic"));
        assert!(prompt.contains("cannot see it"));
        assert!(system_prompt(false, Some("HEIC")).contains(".HEIC"));
        assert!(system_prompt(true, None).contains("folder"));
    }

    #[test]
    fn schema_requires_a_single_name() {
        assert_eq!(schema()["required"][0], "name");
    }
}
