//! Subtitle preview: `.srt` and WebVTT (`.vtt`) cue lists. Read-only.

use serde::Serialize;
use tauri::State;

use crate::app::db::Database;
use crate::preview::source::{decode_text, read_bytes, SOURCE_MAX_BYTES};
use crate::{network, remote};

/// Most cues parsed from one subtitle file.
const MAX_CUES: usize = 5000;

#[derive(Serialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleCue {
    pub index: usize,
    pub start: String,
    pub end: String,
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
}

#[derive(Serialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct SubtitlePreview {
    pub format: String,
    pub cues: Vec<SubtitleCue>,
}

pub(crate) fn is_subtitle_extension(extension: &str) -> bool {
    matches!(
        extension.to_ascii_lowercase().as_str(),
        "srt" | "vtt" | "webvtt"
    )
}

#[tauri::command]
pub async fn open_subtitle(
    database: State<'_, Database>,
    clients: State<'_, remote::RemoteClients>,
    sessions: State<'_, network::servers::Sessions>,
    path: String,
) -> Result<SubtitlePreview, String> {
    let bytes = read_bytes(&database.0, &clients, &sessions, &path, SOURCE_MAX_BYTES).await?;
    let format = if path.to_ascii_lowercase().ends_with(".vtt")
        || path.to_ascii_lowercase().ends_with(".webvtt")
    {
        "vtt"
    } else {
        "srt"
    };
    tauri::async_runtime::spawn_blocking(move || {
        let text = decode_text(&bytes)?;
        parse_subtitle(&text, format)
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Parses `HH:MM:SS,mmm` (SRT) or `HH:MM:SS.mmm` / `MM:SS.mmm` (VTT) into ms.
fn parse_timestamp(value: &str) -> Option<i64> {
    let value = value.trim().replace(',', ".");
    let (clock, millis) = value.split_once('.').unwrap_or((value.as_str(), "0"));
    let parts: Vec<i64> = clock
        .split(':')
        .filter_map(|part| part.parse().ok())
        .collect();
    let (hours, minutes, seconds) = match parts.as_slice() {
        [h, m, s] => (*h, *m, *s),
        [m, s] => (0, *m, *s),
        _ => return None,
    };
    let millis = format!("{millis:0<3}")
        .chars()
        .take(3)
        .collect::<String>()
        .parse()
        .unwrap_or(0);
    Some(hours * 3_600_000 + minutes * 60_000 + seconds * 1000 + millis)
}

/// Strips WebVTT inline markup (`<c.yellow>`, `<v Speaker>`, `<00:00:01.000>`).
fn strip_tags(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut depth = 0;
    for ch in text.chars() {
        match ch {
            '<' => depth += 1,
            '>' if depth > 0 => depth -= 1,
            _ if depth == 0 => out.push(ch),
            _ => {}
        }
    }
    out
}

pub(crate) fn parse_subtitle(text: &str, format: &str) -> Result<SubtitlePreview, String> {
    let mut cues = Vec::new();
    let mut lines = text.lines().peekable();

    // Skip the WEBVTT header and any metadata up to the first blank line.
    if format == "vtt" {
        while let Some(line) = lines.peek() {
            if line.trim().is_empty() {
                lines.next();
                break;
            }
            lines.next();
        }
    }

    while let Some(line) = lines.next() {
        let line = line.trim();
        if line.is_empty()
            || line.starts_with("NOTE")
            || line.starts_with("STYLE")
            || line.starts_with("REGION")
        {
            if line.starts_with("NOTE") {
                while lines.peek().is_some_and(|next| !next.trim().is_empty()) {
                    lines.next();
                }
            }
            continue;
        }
        // Optional numeric id line before the timing line.
        let timing = if line.contains("-->") {
            line.to_string()
        } else {
            match lines.next() {
                Some(next) => next.trim().to_string(),
                None => break,
            }
        };
        let Some((start, end)) = timing.split_once("-->") else {
            continue;
        };
        let (Some(start_ms), Some(end_ms)) = (parse_timestamp(start), parse_timestamp(end)) else {
            continue;
        };
        let mut body = Vec::new();
        while lines.peek().is_some_and(|next| !next.trim().is_empty()) {
            body.push(strip_tags(lines.next().unwrap().trim()).to_string());
        }
        if cues.len() >= MAX_CUES {
            break;
        }
        cues.push(SubtitleCue {
            index: cues.len() + 1,
            start: start.trim().replace(',', "."),
            end: end.trim().replace(',', "."),
            start_ms,
            end_ms,
            text: body.join("\n"),
        });
    }

    if cues.is_empty() {
        return Err("No subtitles found".to_string());
    }
    Ok(SubtitlePreview {
        format: format.to_string(),
        cues,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_srt() {
        let text =
            "1\n00:00:01,000 --> 00:00:04,000\nHola\n\n2\n00:00:05,500 --> 00:00:08,000\nMundo\n";
        let preview = parse_subtitle(text, "srt").unwrap();
        assert_eq!(preview.cues.len(), 2);
        assert_eq!(preview.cues[0].start_ms, 1000);
        assert_eq!(preview.cues[1].start_ms, 5500);
        assert_eq!(preview.cues[1].text, "Mundo");
    }

    #[test]
    fn parses_vtt_and_strips_tags() {
        let text = "WEBVTT\n\n00:00:01.000 --> 00:00:04.000\n<c.yellow>Hola</c> <v Ana>mundo</v>\n";
        let preview = parse_subtitle(text, "vtt").unwrap();
        assert_eq!(preview.cues.len(), 1);
        assert_eq!(preview.cues[0].text, "Hola mundo");
        assert_eq!(preview.cues[0].start_ms, 1000);
    }

    #[test]
    fn errors_without_cues() {
        assert!(parse_subtitle("not subtitles", "srt").is_err());
    }
}
