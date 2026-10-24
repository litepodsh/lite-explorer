//! The rename steps: typed, serializable to JSON, and applied in order to a name.

use chrono::{Datelike, NaiveDateTime, Timelike};
use regex::{NoExpand, RegexBuilder};
use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Target {
    Name,
    Extension,
    Both,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Position {
    Start,
    End,
    /// A character index into the name, clamped to its length.
    Index { at: usize },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CaseMode {
    Upper,
    Lower,
    Title,
    Sentence,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Spaces {
    Keep,
    Dash,
    Underscore,
    Remove,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DateSource {
    Created,
    Modified,
    Exif,
    Today,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum StepKind {
    Replace {
        find: String,
        with: String,
        regex: bool,
        case_sensitive: bool,
        target: Target,
    },
    Insert {
        text: String,
        position: Position,
    },
    Number {
        start: i64,
        step: i64,
        padding: u8,
        separator: String,
        position: Position,
    },
    Case {
        mode: CaseMode,
    },
    Clean {
        trim: bool,
        collapse_spaces: bool,
        strip_accents: bool,
        spaces: Spaces,
    },
    Date {
        source: DateSource,
        format: String,
        separator: String,
        position: Position,
    },
}

fn enabled_by_default() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenameStep {
    #[serde(default = "enabled_by_default")]
    pub enabled: bool,
    #[serde(flatten)]
    pub kind: StepKind,
}

/// A file name split at its last dot. Directories and dotfiles have no extension.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Name {
    pub stem: String,
    pub ext: String,
}

impl Name {
    pub fn split(full: &str, is_dir: bool) -> Name {
        if !is_dir {
            if let Some(dot) = full.rfind('.') {
                if dot > 0 && dot < full.len() - 1 {
                    return Name {
                        stem: full[..dot].to_string(),
                        ext: full[dot + 1..].to_string(),
                    };
                }
            }
        }
        Name {
            stem: full.to_string(),
            ext: String::new(),
        }
    }

    pub fn join(&self) -> String {
        if self.ext.is_empty() {
            self.stem.clone()
        } else {
            format!("{}.{}", self.stem, self.ext)
        }
    }
}

/// Dates a step can ask for. A missing one leaves the name unchanged.
#[derive(Clone, Copy, Debug, Default)]
pub struct Meta {
    pub created: Option<NaiveDateTime>,
    pub modified: Option<NaiveDateTime>,
    pub exif: Option<NaiveDateTime>,
}

/// Whether any enabled step reads the EXIF date, so callers only pay for it when needed.
pub fn needs_exif(steps: &[RenameStep]) -> bool {
    steps.iter().any(|step| {
        step.enabled
            && matches!(
                step.kind,
                StepKind::Date {
                    source: DateSource::Exif,
                    ..
                }
            )
    })
}

fn insert_at(stem: &str, text: &str, position: Position) -> String {
    match position {
        Position::Start => format!("{text}{stem}"),
        Position::End => format!("{stem}{text}"),
        Position::Index { at } => {
            let byte = stem
                .char_indices()
                .nth(at)
                .map_or(stem.len(), |(index, _)| index);
            format!("{}{}{}", &stem[..byte], text, &stem[byte..])
        }
    }
}

/// Joins `text` and `separator` on the side of the name that touches the name.
fn insert_with_separator(stem: &str, text: &str, separator: &str, position: Position) -> String {
    match position {
        Position::Start => format!("{text}{separator}{stem}"),
        Position::End => format!("{stem}{separator}{text}"),
        Position::Index { .. } => insert_at(stem, text, position),
    }
}

fn short_regex_error(error: &regex::Error) -> String {
    let text = error.to_string();
    let line = text.lines().last().unwrap_or("").trim();
    format!("Invalid pattern: {}", line.trim_start_matches("error: "))
}

fn replace_text(
    text: &str,
    find: &str,
    with: &str,
    regex: bool,
    case_sensitive: bool,
) -> Result<String, String> {
    let pattern = if regex {
        find.to_string()
    } else {
        regex::escape(find)
    };
    let compiled = RegexBuilder::new(&pattern)
        .case_insensitive(!case_sensitive)
        .size_limit(1 << 20)
        .build()
        .map_err(|error| short_regex_error(&error))?;
    Ok(if regex {
        compiled.replace_all(text, with).into_owned()
    } else {
        compiled.replace_all(text, NoExpand(with)).into_owned()
    })
}

fn title_case(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut word_start = true;
    for c in text.chars() {
        if c == '\'' || c == '\u{2019}' {
            out.push(c);
        } else if c.is_alphanumeric() {
            if word_start {
                out.extend(c.to_uppercase());
            } else {
                out.extend(c.to_lowercase());
            }
            word_start = false;
        } else {
            out.push(c);
            word_start = true;
        }
    }
    out
}

fn sentence_case(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut first = true;
    for c in text.chars() {
        if first && c.is_alphanumeric() {
            out.extend(c.to_uppercase());
            first = false;
        } else {
            out.extend(c.to_lowercase());
        }
    }
    out
}

fn collapse_spaces(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut previous_space = false;
    for c in text.chars() {
        if c.is_whitespace() {
            if !previous_space {
                out.push(' ');
            }
            previous_space = true;
        } else {
            out.push(c);
            previous_space = false;
        }
    }
    out
}

fn strip_accents(text: &str) -> String {
    text.nfd()
        .filter(|c| !('\u{300}'..='\u{36f}').contains(c))
        .nfc()
        .collect()
}

/// Formats with the tokens `YYYY`, `MM`, `DD`, `HH`, `mm` and `ss`; everything else is kept.
pub fn format_date(date: NaiveDateTime, format: &str) -> String {
    let mut out = String::new();
    let mut rest = format;
    while !rest.is_empty() {
        let token: Option<(usize, String)> = if rest.starts_with("YYYY") {
            Some((4, format!("{:04}", date.year())))
        } else if rest.starts_with("MM") {
            Some((2, format!("{:02}", date.month())))
        } else if rest.starts_with("DD") {
            Some((2, format!("{:02}", date.day())))
        } else if rest.starts_with("HH") {
            Some((2, format!("{:02}", date.hour())))
        } else if rest.starts_with("mm") {
            Some((2, format!("{:02}", date.minute())))
        } else if rest.starts_with("ss") {
            Some((2, format!("{:02}", date.second())))
        } else {
            None
        };
        match token {
            Some((length, text)) => {
                out.push_str(&text);
                rest = &rest[length..];
            }
            None => {
                let c = rest.chars().next().expect("rest is not empty");
                out.push(c);
                rest = &rest[c.len_utf8()..];
            }
        }
    }
    out
}

fn apply_step(
    kind: &StepKind,
    name: &mut Name,
    is_dir: bool,
    index: usize,
    meta: &Meta,
    today: NaiveDateTime,
) -> Result<(), String> {
    match kind {
        StepKind::Replace {
            find,
            with,
            regex,
            case_sensitive,
            target,
        } => {
            if find.is_empty() {
                return Ok(());
            }
            let replace = |text: &str| replace_text(text, find, with, *regex, *case_sensitive);
            match target {
                Target::Name => name.stem = replace(&name.stem)?,
                Target::Extension => name.ext = replace(&name.ext)?,
                Target::Both => *name = Name::split(&replace(&name.join())?, is_dir),
            }
        }
        StepKind::Insert { text, position } => {
            name.stem = insert_at(&name.stem, text, *position);
        }
        StepKind::Number {
            start,
            step,
            padding,
            separator,
            position,
        } => {
            let value = start.saturating_add(step.saturating_mul(index as i64));
            let digits = format!("{:0width$}", value, width = *padding as usize);
            name.stem = insert_with_separator(&name.stem, &digits, separator, *position);
        }
        StepKind::Case { mode } => {
            name.stem = match mode {
                CaseMode::Upper => name.stem.to_uppercase(),
                CaseMode::Lower => name.stem.to_lowercase(),
                CaseMode::Title => title_case(&name.stem),
                CaseMode::Sentence => sentence_case(&name.stem),
            };
        }
        StepKind::Clean {
            trim,
            collapse_spaces: collapse,
            strip_accents: accents,
            spaces,
        } => {
            let mut stem = name.stem.clone();
            if *trim {
                stem = stem.trim().to_string();
            }
            if *collapse {
                stem = collapse_spaces(&stem);
            }
            if *accents {
                stem = strip_accents(&stem);
            }
            let replacement = match spaces {
                Spaces::Keep => None,
                Spaces::Dash => Some("-"),
                Spaces::Underscore => Some("_"),
                Spaces::Remove => Some(""),
            };
            if let Some(replacement) = replacement {
                stem = stem
                    .chars()
                    .map(|c| {
                        if c.is_whitespace() {
                            replacement.to_string()
                        } else {
                            c.to_string()
                        }
                    })
                    .collect();
            }
            name.stem = stem;
        }
        StepKind::Date {
            source,
            format,
            separator,
            position,
        } => {
            let date = match source {
                DateSource::Created => meta.created,
                DateSource::Modified => meta.modified,
                DateSource::Exif => meta.exif,
                DateSource::Today => Some(today),
            };
            if let Some(date) = date {
                let text = format_date(date, format);
                name.stem = insert_with_separator(&name.stem, &text, separator, *position);
            }
        }
    }
    Ok(())
}

/// Applies every enabled step in order. The error carries the index of the failing step.
pub fn apply_all(
    full_name: &str,
    is_dir: bool,
    steps: &[RenameStep],
    index: usize,
    meta: &Meta,
    today: NaiveDateTime,
) -> Result<String, (usize, String)> {
    let mut name = Name::split(full_name, is_dir);
    for (position, step) in steps.iter().enumerate() {
        if step.enabled {
            apply_step(&step.kind, &mut name, is_dir, index, meta, today)
                .map_err(|message| (position, message))?;
        }
    }
    Ok(name.join())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn when(year: i32, month: u32, day: u32, hour: u32, minute: u32, second: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(year, month, day)
            .unwrap()
            .and_hms_opt(hour, minute, second)
            .unwrap()
    }

    fn today() -> NaiveDateTime {
        when(2026, 10, 24, 9, 5, 7)
    }

    fn step(kind: StepKind) -> RenameStep {
        RenameStep {
            enabled: true,
            kind,
        }
    }

    fn run(name: &str, steps: &[RenameStep]) -> String {
        apply_all(name, false, steps, 0, &Meta::default(), today()).unwrap()
    }

    fn replace(find: &str, with: &str, regex: bool, case_sensitive: bool, target: Target) -> RenameStep {
        step(StepKind::Replace {
            find: find.into(),
            with: with.into(),
            regex,
            case_sensitive,
            target,
        })
    }

    #[test]
    fn splits_at_the_last_dot_only() {
        assert_eq!(Name::split("a.tar.gz", false), Name { stem: "a.tar".into(), ext: "gz".into() });
        assert_eq!(Name::split(".gitignore", false).ext, "");
        assert_eq!(Name::split("file.", false).ext, "");
        assert_eq!(Name::split("v1.2", true), Name { stem: "v1.2".into(), ext: String::new() });
        assert_eq!(Name::split("a.tar.gz", false).join(), "a.tar.gz");
    }

    #[test]
    fn replace_plain_text_replaces_every_match_literally() {
        let steps = [replace("a.b", "-", false, true, Target::Name)];
        assert_eq!(run("a.b a.b axb.txt", &steps), "a.b a.b axb.txt".replace("a.b", "-"));
        // The dot is literal: "axb" is untouched.
        assert!(run("axb.txt", &steps).starts_with("axb"));
    }

    #[test]
    fn replace_can_ignore_case() {
        let steps = [replace("img_", "Holiday ", false, false, Target::Name)];
        assert_eq!(run("IMG_001.jpg", &steps), "Holiday 001.jpg");
        let strict = [replace("img_", "Holiday ", false, true, Target::Name)];
        assert_eq!(run("IMG_001.jpg", &strict), "IMG_001.jpg");
    }

    #[test]
    fn replace_regex_supports_groups() {
        let steps = [replace(r"^(\d{4})-(\d{2})-(\d{2})", "$3.$2.$1", true, true, Target::Name)];
        assert_eq!(run("2024-08-12 trip.jpg", &steps), "12.08.2024 trip.jpg");
    }

    #[test]
    fn replace_plain_text_does_not_expand_dollar_signs() {
        let steps = [replace("x", "$1", false, true, Target::Name)];
        assert_eq!(run("axb.txt", &steps), "a$1b.txt");
    }

    #[test]
    fn replace_targets_the_extension_or_the_whole_name() {
        assert_eq!(run("a.jpeg", &[replace("jpeg", "jpg", false, true, Target::Extension)]), "a.jpg");
        assert_eq!(run("jpeg.jpeg", &[replace("jpeg", "jpg", false, true, Target::Name)]), "jpg.jpeg");
        assert_eq!(run("jpeg.jpeg", &[replace("jpeg", "jpg", false, true, Target::Both)]), "jpg.jpg");
    }

    #[test]
    fn replace_with_an_invalid_regex_reports_the_step() {
        let steps = [
            step(StepKind::Case { mode: CaseMode::Lower }),
            replace("(", "x", true, true, Target::Name),
        ];
        let (index, message) = apply_all("A.txt", false, &steps, 0, &Meta::default(), today()).unwrap_err();
        assert_eq!(index, 1);
        assert!(message.starts_with("Invalid pattern"), "{message}");
    }

    #[test]
    fn an_empty_search_does_nothing() {
        assert_eq!(run("a.txt", &[replace("", "x", false, true, Target::Name)]), "a.txt");
    }

    #[test]
    fn insert_goes_before_the_extension() {
        let at = |position| step(StepKind::Insert { text: "X".into(), position });
        assert_eq!(run("abc.txt", &[at(Position::Start)]), "Xabc.txt");
        assert_eq!(run("abc.txt", &[at(Position::End)]), "abcX.txt");
        assert_eq!(run("abc.txt", &[at(Position::Index { at: 1 })]), "aXbc.txt");
        assert_eq!(run("abc.txt", &[at(Position::Index { at: 99 })]), "abcX.txt");
    }

    #[test]
    fn insert_counts_characters_not_bytes() {
        let at = step(StepKind::Insert { text: "-".into(), position: Position::Index { at: 2 } });
        assert_eq!(run("añb.txt", &[at]), "añ-b.txt");
    }

    #[test]
    fn numbering_pads_and_follows_the_list_order() {
        let number = step(StepKind::Number {
            start: 1,
            step: 1,
            padding: 3,
            separator: "_".into(),
            position: Position::End,
        });
        let steps = [number];
        let meta = Meta::default();
        assert_eq!(apply_all("a.jpg", false, &steps, 0, &meta, today()).unwrap(), "a_001.jpg");
        assert_eq!(apply_all("b.jpg", false, &steps, 11, &meta, today()).unwrap(), "b_012.jpg");
    }

    #[test]
    fn numbering_supports_start_step_and_prefix_position() {
        let number = step(StepKind::Number {
            start: 10,
            step: 5,
            padding: 0,
            separator: " - ".into(),
            position: Position::Start,
        });
        let meta = Meta::default();
        assert_eq!(apply_all("a.jpg", false, std::slice::from_ref(&number), 2, &meta, today()).unwrap(), "20 - a.jpg");
    }

    #[test]
    fn case_modes() {
        let case = |mode| [step(StepKind::Case { mode })];
        assert_eq!(run("hello World.TXT", &case(CaseMode::Upper)), "HELLO WORLD.TXT");
        assert_eq!(run("Hello World.txt", &case(CaseMode::Lower)), "hello world.txt");
        assert_eq!(run("hELLO wORLD-trip don't.txt", &case(CaseMode::Title)), "Hello World-Trip Don't.txt");
        assert_eq!(run("hELLO wORLD.txt", &case(CaseMode::Sentence)), "Hello world.txt");
    }

    #[test]
    fn clean_trims_collapses_and_replaces_spaces() {
        let clean = |spaces| {
            [step(StepKind::Clean {
                trim: true,
                collapse_spaces: true,
                strip_accents: false,
                spaces,
            })]
        };
        assert_eq!(run("  my   big  file .txt", &clean(Spaces::Keep)), "my big file.txt");
        assert_eq!(run("my  big file.txt", &clean(Spaces::Dash)), "my-big-file.txt");
        assert_eq!(run("my  big file.txt", &clean(Spaces::Underscore)), "my_big_file.txt");
        assert_eq!(run("my big file.txt", &clean(Spaces::Remove)), "mybigfile.txt");
    }

    #[test]
    fn clean_strips_accents() {
        let clean = [step(StepKind::Clean {
            trim: false,
            collapse_spaces: false,
            strip_accents: true,
            spaces: Spaces::Keep,
        })];
        assert_eq!(run("Canción Ñandú über.txt", &clean), "Cancion Nandu uber.txt");
    }

    #[test]
    fn format_date_replaces_known_tokens_only() {
        let date = when(2024, 8, 2, 14, 3, 9);
        assert_eq!(format_date(date, "YYYY-MM-DD"), "2024-08-02");
        assert_eq!(format_date(date, "YYYYMMDD_HHmmss"), "20240802_140309");
        assert_eq!(format_date(date, "DD/MM/YY"), "02/08/YY");
        assert_eq!(format_date(date, ""), "");
    }

    #[test]
    fn date_steps_use_the_requested_source() {
        let meta = Meta {
            created: Some(when(2020, 1, 2, 0, 0, 0)),
            modified: Some(when(2021, 3, 4, 0, 0, 0)),
            exif: Some(when(2022, 5, 6, 0, 0, 0)),
        };
        let date = |source| {
            [step(StepKind::Date {
                source,
                format: "YYYY-MM-DD".into(),
                separator: " ".into(),
                position: Position::Start,
            })]
        };
        let go = |source| apply_all("a.jpg", false, &date(source), 0, &meta, today()).unwrap();
        assert_eq!(go(DateSource::Created), "2020-01-02 a.jpg");
        assert_eq!(go(DateSource::Modified), "2021-03-04 a.jpg");
        assert_eq!(go(DateSource::Exif), "2022-05-06 a.jpg");
        assert_eq!(go(DateSource::Today), "2026-10-24 a.jpg");
    }

    #[test]
    fn a_missing_date_leaves_the_name_alone() {
        let steps = [step(StepKind::Date {
            source: DateSource::Exif,
            format: "YYYY".into(),
            separator: "-".into(),
            position: Position::End,
        })];
        assert_eq!(run("a.jpg", &steps), "a.jpg");
    }

    #[test]
    fn disabled_steps_are_skipped_and_order_matters() {
        let mut upper = step(StepKind::Case { mode: CaseMode::Upper });
        upper.enabled = false;
        let add = step(StepKind::Insert { text: "x".into(), position: Position::End });
        assert_eq!(run("a.txt", &[upper.clone(), add.clone()]), "ax.txt");
        upper.enabled = true;
        assert_eq!(run("a.txt", &[add.clone(), upper.clone()]), "AX.txt");
        assert_eq!(run("a.txt", &[upper, add]), "Ax.txt");
    }

    #[test]
    fn directories_have_no_extension() {
        let steps = [step(StepKind::Insert { text: "!".into(), position: Position::End })];
        assert_eq!(apply_all("v1.2", true, &steps, 0, &Meta::default(), today()).unwrap(), "v1.2!");
    }

    #[test]
    fn steps_round_trip_through_json() {
        let steps = vec![
            replace("a", "b", true, false, Target::Both),
            step(StepKind::Number {
                start: 1,
                step: 1,
                padding: 2,
                separator: "-".into(),
                position: Position::Index { at: 3 },
            }),
            step(StepKind::Clean {
                trim: true,
                collapse_spaces: false,
                strip_accents: true,
                spaces: Spaces::Dash,
            }),
        ];
        let json = serde_json::to_string(&steps).unwrap();
        assert!(json.contains("\"type\":\"replace\""), "{json}");
        assert!(json.contains("\"caseSensitive\":false"), "{json}");
        assert!(json.contains("\"collapseSpaces\":false"), "{json}");
        assert!(json.contains("\"position\":{\"kind\":\"index\",\"at\":3}"), "{json}");
        assert_eq!(serde_json::from_str::<Vec<RenameStep>>(&json).unwrap(), steps);
    }

    #[test]
    fn enabled_defaults_to_true_when_missing() {
        let step: RenameStep = serde_json::from_str(r#"{"type":"case","mode":"upper"}"#).unwrap();
        assert!(step.enabled);
    }

    #[test]
    fn exif_is_only_needed_when_an_enabled_step_asks_for_it() {
        let mut date = step(StepKind::Date {
            source: DateSource::Exif,
            format: "YYYY".into(),
            separator: "-".into(),
            position: Position::Start,
        });
        assert!(needs_exif(std::slice::from_ref(&date)));
        date.enabled = false;
        assert!(!needs_exif(&[date]));
        assert!(!needs_exif(&[step(StepKind::Case { mode: CaseMode::Upper })]));
    }
}
