//! Spreadsheet preview: reads `.xlsx`, `.xls`, `.xlsb` and OpenDocument (`.ods`)
//! workbooks through calamine and hands the frontend a bounded grid per sheet.
//! Read-only: formulas appear as their cached values, never evaluated.

use std::{fs, io::Cursor, path::Path};

use calamine::{Data, DataType, Reader};
use serde::Serialize;
use tauri::State;

use crate::app::db::Database;
use crate::explorer::local_path::{validate_existing, ExpectedKind};
use crate::preview::xml::{self, Element};
use crate::search::text;
use crate::{network, remote};

/// Largest workbook read for preview, matching content search's spreadsheet cap.
pub(crate) const SHEET_MAX_BYTES: usize = 32 * 1024 * 1024;
/// Per-sheet preview bounds; anything past them is reported as truncated.
const MAX_SHEETS: usize = 32;
const MAX_ROWS: usize = 2000;
const MAX_COLUMNS: usize = 128;

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct SheetTable {
    pub name: String,
    pub rows: Vec<Vec<String>>,
    pub truncated: bool,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct SpreadsheetData {
    pub sheets: Vec<SheetTable>,
    pub truncated: bool,
}

/// Extensions calamine reads as a spreadsheet workbook, plus flat OpenDocument.
pub(crate) fn is_spreadsheet_extension(extension: &str) -> bool {
    matches!(
        extension.to_ascii_lowercase().as_str(),
        "xlsx"
            | "xlsm"
            | "xltx"
            | "xltm"
            | "xls"
            | "xla"
            | "xlam"
            | "xlsb"
            | "ods"
            | "ots"
            | "fods"
    )
}

/// Reads a workbook from the local filesystem.
pub(crate) fn read_spreadsheet_file(path: &Path) -> Result<SpreadsheetData, String> {
    let metadata = fs::metadata(path).map_err(|error| error.to_string())?;
    if metadata.len() > SHEET_MAX_BYTES as u64 {
        return Err(too_large());
    }
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    parse_spreadsheet(bytes)
}

/// Parses a workbook already in memory, shared by the local, remote and server paths.
pub(crate) fn parse_spreadsheet(bytes: Vec<u8>) -> Result<SpreadsheetData, String> {
    if bytes.len() > SHEET_MAX_BYTES {
        return Err(too_large());
    }
    // Flat OpenDocument (`.fods`) is a single XML document, not a zip package.
    if looks_like_xml(&bytes) {
        let xml = text::decode(&bytes).ok_or_else(|| "Unreadable spreadsheet".to_string())?;
        return parse_flat_spreadsheet(&xml);
    }
    let mut workbook = calamine::open_workbook_auto_from_rs(Cursor::new(bytes))
        .map_err(|error| error.to_string())?;
    let names = workbook.sheet_names().to_vec();
    let mut truncated = names.len() > MAX_SHEETS;
    let mut sheets = Vec::new();
    for name in names.into_iter().take(MAX_SHEETS) {
        let range = workbook
            .worksheet_range(&name)
            .map_err(|error| error.to_string())?;
        let sheet_truncated = range.height() > MAX_ROWS || range.width() > MAX_COLUMNS;
        truncated |= sheet_truncated;
        let mut rows: Vec<Vec<String>> = range
            .rows()
            .take(MAX_ROWS)
            .map(|row| row.iter().take(MAX_COLUMNS).map(cell_text).collect())
            .collect();
        trim_trailing_empty(&mut rows);
        sheets.push(SheetTable {
            name,
            rows,
            truncated: sheet_truncated,
        });
    }
    Ok(SpreadsheetData { sheets, truncated })
}

/// True when the bytes open an XML document rather than a zip workbook.
fn looks_like_xml(bytes: &[u8]) -> bool {
    let start = bytes
        .iter()
        .position(|byte| {
            !byte.is_ascii_whitespace() && *byte != 0xEF && *byte != 0xBB && *byte != 0xBF
        })
        .unwrap_or(bytes.len());
    bytes[start..].starts_with(b"<?xml") || bytes[start..].starts_with(b"<office:")
}

/// Drops the trailing all-empty rows and per-row trailing empty cells the grid does
/// not need to render, keeping the payload small for sparse sheets.
fn trim_trailing_empty(rows: &mut Vec<Vec<String>>) {
    while rows
        .last()
        .is_some_and(|row| row.iter().all(String::is_empty))
    {
        rows.pop();
    }
    for row in rows {
        while row.last().is_some_and(String::is_empty) {
            row.pop();
        }
    }
}

/// Flat OpenDocument spreadsheet (`.fods`): `<table:table>` per sheet with rows and
/// cells, honoring `number-columns-repeated` / `number-rows-repeated`.
fn parse_flat_spreadsheet(xml_text: &str) -> Result<SpreadsheetData, String> {
    let document = xml::parse_document(xml_text);
    let spreadsheet = document
        .descendant("spreadsheet")
        .ok_or("Not a flat spreadsheet")?;
    let mut sheets = Vec::new();
    let mut truncated = false;
    for table in spreadsheet.children_named("table") {
        if sheets.len() >= MAX_SHEETS {
            truncated = true;
            break;
        }
        let name = table.attr("name").unwrap_or("Sheet").to_string();
        let mut rows: Vec<Vec<String>> = Vec::new();
        let mut sheet_truncated = false;
        'rows: for row in table.children_named("table-row") {
            let row_repeat = repeated(row.attr("number-rows-repeated"));
            let mut cells: Vec<String> = Vec::new();
            for cell in row
                .children
                .iter()
                .filter(|cell| matches!(cell.local(), "table-cell" | "covered-table-cell"))
            {
                let value = flat_cell_text(cell);
                for _ in 0..repeated(cell.attr("number-columns-repeated")) {
                    if cells.len() >= MAX_COLUMNS {
                        sheet_truncated = true;
                        break;
                    }
                    cells.push(value.clone());
                }
            }
            for _ in 0..row_repeat {
                if rows.len() >= MAX_ROWS {
                    sheet_truncated = true;
                    break 'rows;
                }
                rows.push(cells.clone());
            }
        }
        trim_trailing_empty(&mut rows);
        truncated |= sheet_truncated;
        sheets.push(SheetTable {
            name,
            rows,
            truncated: sheet_truncated,
        });
    }
    Ok(SpreadsheetData { sheets, truncated })
}

fn repeated(value: Option<&str>) -> usize {
    value
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|&value| value > 0)
        .unwrap_or(1)
}

/// Cell display text: the shown `<text:p>` when present, else the typed value.
fn flat_cell_text(cell: &Element) -> String {
    let text = cell.text_content();
    let text = text.trim();
    if !text.is_empty() {
        return text.to_string();
    }
    match cell.attr("value-type") {
        Some("boolean") => match cell.attr("boolean-value") {
            Some("true") => "TRUE".into(),
            Some(_) => "FALSE".into(),
            None => String::new(),
        },
        Some("date") => cell
            .attr("date-value")
            .map(|value| value.split('T').next().unwrap_or(value).to_string())
            .unwrap_or_default(),
        Some(_) => cell.attr("value").unwrap_or("").to_string(),
        None => String::new(),
    }
}

fn too_large() -> String {
    format!(
        "Spreadsheet is larger than {} MB",
        SHEET_MAX_BYTES / (1024 * 1024)
    )
}

/// One cell as display text. Dates become ISO strings; an all-zero time shows the
/// date only. Formulas keep the value the file cached, not the formula itself.
fn cell_text(cell: &Data) -> String {
    match cell {
        Data::Empty => String::new(),
        Data::DateTime(_) => cell
            .as_datetime()
            .map(|value| {
                if value.time() == chrono::NaiveTime::MIN {
                    value.format("%Y-%m-%d").to_string()
                } else {
                    value.format("%Y-%m-%d %H:%M:%S").to_string()
                }
            })
            .unwrap_or_else(|| cell.to_string()),
        _ => cell.to_string(),
    }
}

#[tauri::command]
pub async fn read_spreadsheet(
    database: State<'_, Database>,
    clients: State<'_, remote::RemoteClients>,
    sessions: State<'_, network::servers::Sessions>,
    path: String,
) -> Result<SpreadsheetData, String> {
    if network::servers::is_server_path(&path) {
        return network::servers::read_spreadsheet(&database.0, &sessions, &path).await;
    }
    if remote::is_remote_path(&path) {
        return remote::read_spreadsheet(&database.0, &clients, &path).await;
    }
    read_spreadsheet_file(&validate_existing(Path::new(&path), ExpectedKind::File)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::{write::SimpleFileOptions, ZipWriter};

    fn zip_bytes(parts: &[(&str, &str)]) -> Vec<u8> {
        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        for (name, body) in parts {
            zip.start_file(*name, SimpleFileOptions::default()).unwrap();
            zip.write_all(body.as_bytes()).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }

    fn workbook() -> Vec<u8> {
        zip_bytes(&[
            (
                "[Content_Types].xml",
                r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/worksheets/sheet2.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>"#,
            ),
            (
                "_rels/.rels",
                r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#,
            ),
            (
                "xl/workbook.xml",
                r#"<?xml version="1.0"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Data" sheetId="1" r:id="rId1"/><sheet name="Notes" sheetId="2" r:id="rId2"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet2.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<?xml version="1.0"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>Name</t></is></c><c r="B1" t="inlineStr"><is><t>Total</t></is></c></row><row r="2"><c r="A2" t="inlineStr"><is><t>Zarpa API</t></is></c><c r="B2"><v>4217.5</v></c></row><row r="3"/></sheetData></worksheet>"#,
            ),
            (
                "xl/worksheets/sheet2.xml",
                r#"<?xml version="1.0"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>Hola</t></is></c></row></sheetData></worksheet>"#,
            ),
        ])
    }

    #[test]
    fn detects_spreadsheet_extensions() {
        assert!(is_spreadsheet_extension("XLSX"));
        assert!(is_spreadsheet_extension("ods"));
        assert!(is_spreadsheet_extension("xlsb"));
        assert!(is_spreadsheet_extension("fods"));
        assert!(!is_spreadsheet_extension("csv"));
    }

    #[test]
    fn parses_flat_opendocument_with_repeats() {
        let fods = concat!(
            r#"<?xml version="1.0" encoding="UTF-8"?>"#,
            r#"<office:document office:mimetype="application/vnd.oasis.opendocument.spreadsheet">"#,
            r#"<office:body><office:spreadsheet>"#,
            r#"<table:table table:name="Ventas"><table:table-row>"#,
            r#"<table:table-cell office:value-type="string"><text:p>Región</text:p></table:table-cell>"#,
            r#"<table:table-cell office:value-type="string"><text:p>Unidades</text:p></table:table-cell>"#,
            r#"</table:table-row><table:table-row>"#,
            r#"<table:table-cell office:value-type="string"><text:p>Bogotá &amp; Cía</text:p></table:table-cell>"#,
            r#"<table:table-cell office:value-type="float" office:value="632"><text:p>632</text:p></table:table-cell>"#,
            r#"<table:table-cell table:number-columns-repeated="2"/>"#,
            r#"</table:table-row>"#,
            r#"<table:table-row table:number-rows-repeated="1048576"/>"#,
            r#"</table:table></office:spreadsheet></office:body></office:document>"#,
        );

        let data = parse_spreadsheet(fods.as_bytes().to_vec()).unwrap();

        assert_eq!(data.sheets.len(), 1);
        assert_eq!(data.sheets[0].name, "Ventas");
        assert_eq!(
            data.sheets[0].rows,
            vec![
                vec!["Región".to_string(), "Unidades".to_string()],
                vec!["Bogotá & Cía".to_string(), "632".to_string()],
            ]
        );
    }

    #[test]
    fn parses_every_sheet_into_trimmed_rows() {
        let data = parse_spreadsheet(workbook()).unwrap();

        assert_eq!(data.sheets.len(), 2);
        assert!(!data.truncated);

        let data_sheet = &data.sheets[0];
        assert_eq!(data_sheet.name, "Data");
        assert_eq!(
            data_sheet.rows,
            vec![
                vec!["Name".to_string(), "Total".to_string()],
                vec!["Zarpa API".to_string(), "4217.5".to_string()],
            ]
        );
        assert!(!data_sheet.truncated);

        assert_eq!(data.sheets[1].rows, vec![vec!["Hola".to_string()]]);
    }

    #[test]
    fn rejects_bytes_that_are_not_a_workbook() {
        assert!(parse_spreadsheet(b"not a spreadsheet".to_vec()).is_err());
    }
}
