//! iCalendar (`.ics`) preview: parses events, todos and journal entries from a
//! calendar file. Read-only; recurrence rules aren't expanded.

use serde::Serialize;
use tauri::State;

use crate::app::db::Database;
use crate::preview::source::{decode_text, read_bytes, SOURCE_MAX_BYTES};
use crate::{network, remote};

#[derive(Serialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct CalendarEvent {
    pub kind: String,
    pub summary: String,
    pub start: Option<String>,
    pub end: Option<String>,
    pub all_day: bool,
    pub location: String,
    pub description: String,
    pub organizer: String,
    pub uid: String,
}

#[derive(Serialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct CalendarPreview {
    pub name: String,
    pub events: Vec<CalendarEvent>,
}

pub(crate) fn is_calendar_extension(extension: &str) -> bool {
    matches!(
        extension.to_ascii_lowercase().as_str(),
        "ics" | "ifb" | "ical"
    )
}

#[tauri::command]
pub async fn open_calendar(
    database: State<'_, Database>,
    clients: State<'_, remote::RemoteClients>,
    sessions: State<'_, network::servers::Sessions>,
    path: String,
) -> Result<CalendarPreview, String> {
    let bytes = read_bytes(&database.0, &clients, &sessions, &path, SOURCE_MAX_BYTES).await?;
    tauri::async_runtime::spawn_blocking(move || {
        let text = decode_text(&bytes)?;
        parse_calendar(&text)
    })
    .await
    .map_err(|error| error.to_string())?
}

fn unfold(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for raw in text.split('\n') {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        if line.starts_with([' ', '\t']) {
            if let Some(last) = lines.last_mut() {
                last.push_str(&line[1..]);
                continue;
            }
        }
        lines.push(line.to_string());
    }
    lines
}

fn unescape(value: &str) -> String {
    value
        .replace("\\n", "\n")
        .replace("\\N", "\n")
        .replace("\\,", ",")
        .replace("\\;", ";")
        .replace("\\\\", "\\")
}

/// Splits `DTSTART;TZID=...:20260921T090000` into key, params and value.
fn split_property(line: &str) -> Option<(String, Vec<String>, String)> {
    let colon = line.find(':')?;
    let head = &line[..colon];
    let value = line[colon + 1..].to_string();
    let mut parts = head.split(';');
    let key = parts.next()?.trim().to_ascii_uppercase();
    let params = parts.map(|part| part.trim().to_ascii_uppercase()).collect();
    Some((key, params, value))
}

fn format_date(value: &str, all_day: bool) -> String {
    let value = value.trim();
    if all_day || value.len() == 8 {
        if value.len() >= 8 {
            return format!("{}-{}-{}", &value[0..4], &value[4..6], &value[6..8]);
        }
        return value.to_string();
    }
    let utc = value.ends_with('Z');
    let core = value.trim_end_matches('Z');
    if core.len() >= 15 && core.as_bytes().get(8) == Some(&b'T') {
        return format!(
            "{}-{}-{} {}:{}",
            &core[0..4],
            &core[4..6],
            &core[6..8],
            &core[9..11],
            &core[11..13]
        ) + if utc { " UTC" } else { "" };
    }
    value.to_string()
}

pub(crate) fn parse_calendar(text: &str) -> Result<CalendarPreview, String> {
    let mut preview = CalendarPreview::default();
    let mut current: Option<CalendarEvent> = None;

    for line in unfold(text) {
        let trimmed = line.trim();
        if trimmed.eq_ignore_ascii_case("BEGIN:VEVENT") {
            current = Some(CalendarEvent {
                kind: "event".to_string(),
                ..Default::default()
            });
            continue;
        }
        if trimmed.eq_ignore_ascii_case("BEGIN:VTODO") {
            current = Some(CalendarEvent {
                kind: "todo".to_string(),
                ..Default::default()
            });
            continue;
        }
        if trimmed.eq_ignore_ascii_case("BEGIN:VJOURNAL") {
            current = Some(CalendarEvent {
                kind: "journal".to_string(),
                ..Default::default()
            });
            continue;
        }
        if trimmed.eq_ignore_ascii_case("END:VEVENT")
            || trimmed.eq_ignore_ascii_case("END:VTODO")
            || trimmed.eq_ignore_ascii_case("END:VJOURNAL")
        {
            if let Some(event) = current.take() {
                preview.events.push(event);
            }
            continue;
        }
        if preview.name.is_empty() && trimmed.to_ascii_uppercase().starts_with("X-WR-CALNAME") {
            if let Some((_, _, value)) = split_property(trimmed) {
                preview.name = value.trim().to_string();
            }
            continue;
        }
        let Some(event) = current.as_mut() else {
            continue;
        };
        let Some((key, params, value)) = split_property(trimmed) else {
            continue;
        };
        if value.is_empty() {
            continue;
        }
        match key.as_str() {
            "SUMMARY" => event.summary = unescape(&value),
            "LOCATION" => event.location = unescape(&value),
            "DESCRIPTION" => event.description = unescape(&value),
            "UID" => event.uid = value,
            "ORGANIZER" => {
                event.organizer = value
                    .trim_start_matches("mailto:")
                    .trim_start_matches("MAILTO:")
                    .to_string()
            }
            "DTSTART" => {
                let all_day = params.iter().any(|param| param == "VALUE=DATE");
                event.all_day = all_day;
                event.start = Some(format_date(&value, all_day));
            }
            "DTEND" => {
                event.end = Some(format_date(&value, event.all_day));
            }
            "DUE" => {
                let all_day = params.iter().any(|param| param == "VALUE=DATE");
                event.all_day = event.all_day || all_day;
                event.end = Some(format_date(&value, all_day));
            }
            _ => {}
        }
    }

    if preview.events.is_empty() {
        return Err("No events found in calendar".to_string());
    }
    Ok(preview)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nX-WR-CALNAME:Agenda\r\nBEGIN:VEVENT\r\nUID:1\r\nDTSTART:20260921T140000Z\r\nDTEND:20260921T150000Z\r\nSUMMARY:Revisión\r\nLOCATION:Sala 3\r\nDESCRIPTION:Línea 1\\nLínea 2\r\nORGANIZER;CN=Ana:mailto:ana@lite.dev\r\nEND:VEVENT\r\nBEGIN:VTODO\r\nUID:2\r\nDUE;VALUE=DATE:20260930\r\nSUMMARY:Renovar\r\nEND:VTODO\r\nEND:VCALENDAR\r\n";

    #[test]
    fn parses_events_and_todos() {
        let calendar = parse_calendar(SAMPLE).unwrap();
        assert_eq!(calendar.name, "Agenda");
        assert_eq!(calendar.events.len(), 2);
        let event = &calendar.events[0];
        assert_eq!(event.summary, "Revisión");
        assert_eq!(event.start.as_deref(), Some("2026-09-21 14:00 UTC"));
        assert_eq!(event.end.as_deref(), Some("2026-09-21 15:00 UTC"));
        assert_eq!(event.location, "Sala 3");
        assert_eq!(event.description, "Línea 1\nLínea 2");
        assert_eq!(event.organizer, "ana@lite.dev");
        let todo = &calendar.events[1];
        assert_eq!(todo.kind, "todo");
        assert!(todo.all_day);
        assert_eq!(todo.end.as_deref(), Some("2026-09-30"));
    }

    #[test]
    fn errors_without_events() {
        assert!(parse_calendar("BEGIN:VCALENDAR\nEND:VCALENDAR").is_err());
    }
}
