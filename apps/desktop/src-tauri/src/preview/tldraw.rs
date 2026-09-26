//! Read-only tldraw desktop archives and JSON exports. Archived scripts are never loaded.

use std::{
    collections::BTreeMap,
    io::{Cursor, Read, Write},
};

use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Serialize;
use serde_json::{json, Map, Value};
use sqlx::{sqlite::SqliteConnectOptions, Connection, SqliteConnection};
use tauri::State;
use tempfile::NamedTempFile;
use zip::ZipArchive;

use super::source::{read_bytes, SOURCE_MAX_BYTES};
use crate::{app::db::Database, network, remote};

const MAX_RECORDS: usize = 10_000;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TldrawPreview {
    document: Value,
    current_page_id: Option<String>,
    assets: BTreeMap<String, String>,
}

pub(crate) fn is_tldraw_extension(extension: &str) -> bool {
    extension.eq_ignore_ascii_case("tldraw") || extension.eq_ignore_ascii_case("tldr")
}

#[tauri::command]
pub async fn open_tldraw(
    database: State<'_, Database>,
    clients: State<'_, remote::RemoteClients>,
    sessions: State<'_, network::servers::Sessions>,
    path: String,
) -> Result<TldrawPreview, String> {
    let bytes = read_bytes(&database.0, &clients, &sessions, &path, SOURCE_MAX_BYTES).await?;
    parse_tldraw(bytes)
        .await
        .map_err(|error| format!("Can’t preview this tldraw document: {error}"))
}

fn snapshot(schema: Value, records: Vec<Value>) -> Result<Value, String> {
    if !schema.is_object() || records.len() > MAX_RECORDS {
        return Err("Invalid schema or more than 10,000 records".into());
    }
    let mut store = Map::new();
    for record in records {
        let id = record
            .get("id")
            .and_then(Value::as_str)
            .ok_or("Record has no ID")?
            .to_owned();
        if !record.get("typeName").is_some_and(Value::is_string) {
            return Err("Record has no type".into());
        }
        if store.insert(id, record).is_some() {
            return Err("Duplicate record ID".into());
        }
    }
    Ok(json!({ "store": store, "schema": schema }))
}

fn read_entry(archive: &mut ZipArchive<Cursor<Vec<u8>>>, name: &str) -> Result<Vec<u8>, String> {
    let entry = archive
        .by_name(name)
        .map_err(|_| format!("Missing {name}"))?;
    if entry.size() > SOURCE_MAX_BYTES as u64 {
        return Err("Archive entry exceeds the 32 MB preview limit".into());
    }
    let mut bytes = Vec::new();
    entry
        .take(SOURCE_MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > SOURCE_MAX_BYTES {
        return Err("Archive entry exceeds the 32 MB preview limit".into());
    }
    Ok(bytes)
}

fn unpack(
    bytes: Vec<u8>,
) -> Result<(NamedTempFile, Option<String>, BTreeMap<String, String>), String> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(|error| error.to_string())?;
    if archive.len() > MAX_RECORDS {
        return Err("Too many archive entries".into());
    }
    let metadata: Value = serde_json::from_slice(&read_entry(&mut archive, "metadata.json")?)
        .map_err(|error| error.to_string())?;
    if metadata["formatVersion"] != 1 {
        return Err("Unsupported tldraw archive version".into());
    }
    let names: Vec<String> = archive.file_names().map(str::to_owned).collect();
    let mut expanded = 0u64;
    for name in &names {
        expanded = expanded
            .checked_add(
                archive
                    .by_name(name)
                    .map_err(|error| error.to_string())?
                    .size(),
            )
            .ok_or("Archive is too large")?;
        if expanded > SOURCE_MAX_BYTES as u64 {
            return Err("Expanded archive exceeds the 32 MB preview limit".into());
        }
    }
    let current_page_id = if names.iter().any(|name| name == "session.json") {
        let session: Value = serde_json::from_slice(&read_entry(&mut archive, "session.json")?)
            .map_err(|error| error.to_string())?;
        session["currentPageId"].as_str().map(str::to_owned)
    } else {
        None
    };
    let mut assets = BTreeMap::new();
    for name in names
        .iter()
        .filter(|name| name.starts_with("assets/") && !name.ends_with('/'))
    {
        // Entries are read directly, never extracted using untrusted paths.
        assets.insert(
            name.clone(),
            STANDARD.encode(read_entry(&mut archive, name)?),
        );
    }
    let database = read_entry(&mut archive, "db.sqlite")?;
    if !database.starts_with(b"SQLite format 3\0") {
        return Err("Invalid SQLite document".into());
    }
    let mut file = NamedTempFile::new().map_err(|error| error.to_string())?;
    file.write_all(&database)
        .map_err(|error| error.to_string())?;
    Ok((file, current_page_id, assets))
}

async fn parse_tldraw(bytes: Vec<u8>) -> Result<TldrawPreview, String> {
    if bytes.len() > SOURCE_MAX_BYTES {
        return Err("Document exceeds the 32 MB preview limit".into());
    }
    if !bytes.starts_with(b"PK\x03\x04") {
        let mut value: Value = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
        if value["tldrawFileFormatVersion"] != 1 {
            return Err("Unsupported tldraw JSON format".into());
        }
        let records = value["records"]
            .take()
            .as_array()
            .ok_or("Missing records")?
            .clone();
        return Ok(TldrawPreview {
            document: snapshot(value["schema"].take(), records)?,
            current_page_id: None,
            assets: BTreeMap::new(),
        });
    }
    let (file, current_page_id, assets) =
        tauri::async_runtime::spawn_blocking(move || unpack(bytes))
            .await
            .map_err(|error| error.to_string())??;
    let options = SqliteConnectOptions::new()
        .filename(file.path())
        .read_only(true)
        .immutable(true)
        .pragma("trusted_schema", "OFF");
    let mut connection = SqliteConnection::connect_with(&options)
        .await
        .map_err(|error| error.to_string())?;
    let result = async {
        // Do not execute views supplied by a document in place of these tables.
        let tables: i64 = sqlx::query_scalar("SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name IN ('documents', 'metadata')")
            .fetch_one(&mut connection).await.map_err(|error| error.to_string())?;
        if tables != 2 { return Err("Missing document tables".into()); }
        let schema: String = sqlx::query_scalar("SELECT schema FROM metadata LIMIT 1").fetch_one(&mut connection).await.map_err(|error| error.to_string())?;
        let rows: Vec<String> = sqlx::query_scalar("SELECT CAST(state AS TEXT) FROM documents LIMIT 10001").fetch_all(&mut connection).await.map_err(|error| error.to_string())?;
        if rows.len() > MAX_RECORDS || rows.iter().map(String::len).sum::<usize>() > SOURCE_MAX_BYTES {
            return Err("Document exceeds the preview record limit".into());
        }
        let records = rows.iter().map(|row| serde_json::from_str(row)).collect::<Result<Vec<Value>, _>>().map_err(|error| error.to_string())?;
        snapshot(serde_json::from_str(&schema).map_err(|error| error.to_string())?, records)
    }.await;
    connection
        .close()
        .await
        .map_err(|error| error.to_string())?;
    Ok(TldrawPreview {
        document: result?,
        current_page_id,
        assets,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn reads_archive_and_json_and_rejects_invalid_documents() {
        let file = NamedTempFile::new().unwrap();
        let mut db = SqliteConnection::connect_with(
            &SqliteConnectOptions::new()
                .filename(file.path())
                .create_if_missing(true),
        )
        .await
        .unwrap();
        sqlx::query("CREATE TABLE metadata (schema TEXT); CREATE TABLE documents (state BLOB)")
            .execute(&mut db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO metadata VALUES (?)")
            .bind(r#"{"schemaVersion":2,"sequences":{}}"#)
            .execute(&mut db)
            .await
            .unwrap();
        let record = json!({"id":"page:one","typeName":"page","name":"Page 1"});
        sqlx::query("INSERT INTO documents VALUES (?)")
            .bind(record.to_string().into_bytes())
            .execute(&mut db)
            .await
            .unwrap();
        db.close().await.unwrap();
        let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, bytes) in [
            ("metadata.json", br#"{"formatVersion":1}"#.to_vec()),
            ("db.sqlite", std::fs::read(file.path()).unwrap()),
            ("session.json", br#"{"currentPageId":"page:one"}"#.to_vec()),
            ("assets/example", b"image bytes".to_vec()),
        ] {
            archive
                .start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            archive.write_all(&bytes).unwrap();
        }
        let preview = parse_tldraw(archive.finish().unwrap().into_inner())
            .await
            .unwrap();
        assert_eq!(preview.document["store"]["page:one"], record);
        assert_eq!(preview.current_page_id.as_deref(), Some("page:one"));
        assert_eq!(
            preview.assets["assets/example"],
            STANDARD.encode(b"image bytes")
        );
        let json = json!({"tldrawFileFormatVersion":1,"schema":preview.document["schema"],"records":[record]});
        assert_eq!(
            parse_tldraw(json.to_string().into_bytes())
                .await
                .unwrap()
                .document,
            preview.document
        );
        for bytes in [
            b"not a document".to_vec(),
            b"PK\x03\x04broken".to_vec(),
            b"{}".to_vec(),
            vec![0; SOURCE_MAX_BYTES + 1],
        ] {
            assert!(parse_tldraw(bytes).await.is_err());
        }
        assert!(snapshot(json!({}), vec![json!({"id":"x"})]).is_err());
        assert!(is_tldraw_extension("TLDRAW"));
        assert!(is_tldraw_extension("tldr"));
        assert!(!is_tldraw_extension("zip"));
    }

    #[tokio::test]
    #[ignore = "Set TLDRAW_PREVIEW_FIXTURE to a local document to check compatibility"]
    async fn opens_real_document() {
        let path = std::env::var("TLDRAW_PREVIEW_FIXTURE").unwrap();
        let preview = parse_tldraw(std::fs::read(path).unwrap()).await.unwrap();
        assert!(preview.document["store"]
            .as_object()
            .unwrap()
            .values()
            .any(|record| record["typeName"] == "page"));
    }
}
