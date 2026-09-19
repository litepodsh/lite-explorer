//! Geo preview: `.geojson`, `.kml` and `.gpx` are normalized into simple features
//! the frontend plots on an offline SVG map. Read-only.

use serde::Serialize;
use tauri::State;

use crate::app::db::Database;
use crate::preview::source::{decode_text, read_bytes, SOURCE_MAX_BYTES};
use crate::preview::xml::parse_document;
use crate::{network, remote};

/// Most features parsed from one file.
const MAX_FEATURES: usize = 5000;

#[derive(Serialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct GeoFeature {
    pub name: String,
    /// `Point`, `LineString` or `Polygon`.
    pub kind: String,
    /// Flat list of `[lon, lat]` points (polygon = outer ring).
    pub points: Vec<[f64; 2]>,
}

#[derive(Serialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct GeoPreview {
    pub format: String,
    pub features: Vec<GeoFeature>,
    /// `[minLon, minLat, maxLon, maxLat]`.
    pub bounds: Option<[f64; 4]>,
    pub point_count: usize,
}

pub(crate) fn is_geo_extension(extension: &str) -> bool {
    matches!(
        extension.to_ascii_lowercase().as_str(),
        "geojson" | "kml" | "gpx"
    )
}

#[tauri::command]
pub async fn open_geo(
    database: State<'_, Database>,
    clients: State<'_, remote::RemoteClients>,
    sessions: State<'_, network::servers::Sessions>,
    path: String,
) -> Result<GeoPreview, String> {
    let bytes = read_bytes(&database.0, &clients, &sessions, &path, SOURCE_MAX_BYTES).await?;
    let extension = std::path::Path::new(&path)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    tauri::async_runtime::spawn_blocking(move || {
        let text = decode_text(&bytes)?;
        parse_geo(&text, &extension)
    })
    .await
    .map_err(|error| error.to_string())?
}

fn to_point(value: &serde_json::Value) -> Option<[f64; 2]> {
    let array = value.as_array()?;
    Some([array.first()?.as_f64()?, array.get(1)?.as_f64()?])
}

fn parse_geojson(text: &str) -> Result<GeoPreview, String> {
    let root: serde_json::Value =
        serde_json::from_str(text).map_err(|error| format!("Invalid GeoJSON: {error}"))?;
    let mut features = Vec::new();

    let items: Vec<&serde_json::Value> = match root.get("type").and_then(serde_json::Value::as_str)
    {
        Some("FeatureCollection") => root
            .get("features")
            .and_then(serde_json::Value::as_array)
            .map(|items| items.iter().collect())
            .unwrap_or_default(),
        Some("Feature") => vec![&root],
        _ => vec![&root],
    };

    for item in items {
        if features.len() >= MAX_FEATURES {
            break;
        }
        let name = item
            .get("properties")
            .and_then(|properties| properties.get("name"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_string();
        let geometry = item.get("geometry").unwrap_or(item);
        let Some(kind) = geometry.get("type").and_then(serde_json::Value::as_str) else {
            continue;
        };
        let coordinates = geometry.get("coordinates").cloned().unwrap_or_default();
        for feature in geometry_features(kind, &coordinates, &name) {
            features.push(feature);
            if features.len() >= MAX_FEATURES {
                break;
            }
        }
    }
    finalize("geojson", features)
}

/// Maps a GeoJSON geometry to one or more normalized features.
fn geometry_features(kind: &str, coordinates: &serde_json::Value, name: &str) -> Vec<GeoFeature> {
    let mut out = Vec::new();
    match kind {
        "Point" => {
            if let Some(point) = to_point(coordinates) {
                out.push(GeoFeature {
                    name: name.to_string(),
                    kind: "Point".to_string(),
                    points: vec![point],
                });
            }
        }
        "MultiPoint" => {
            let points = coordinates
                .as_array()
                .map(|items| items.iter().filter_map(to_point).collect::<Vec<_>>())
                .unwrap_or_default();
            if !points.is_empty() {
                out.push(GeoFeature {
                    name: name.to_string(),
                    kind: "Point".to_string(),
                    points,
                });
            }
        }
        "LineString" | "MultiLineString" => {
            let points = flatten_lines(coordinates);
            if !points.is_empty() {
                out.push(GeoFeature {
                    name: name.to_string(),
                    kind: "LineString".to_string(),
                    points,
                });
            }
        }
        "Polygon" => {
            if let Some(points) = outer_ring(coordinates) {
                out.push(GeoFeature {
                    name: name.to_string(),
                    kind: "Polygon".to_string(),
                    points,
                });
            }
        }
        "MultiPolygon" => {
            if let Some(polygons) = coordinates.as_array() {
                for polygon in polygons {
                    if let Some(points) = outer_ring(polygon) {
                        out.push(GeoFeature {
                            name: name.to_string(),
                            kind: "Polygon".to_string(),
                            points,
                        });
                    }
                }
            }
        }
        _ => {}
    }
    out
}

fn flatten_lines(value: &serde_json::Value) -> Vec<[f64; 2]> {
    let mut points = Vec::new();
    collect_lines(value, &mut points);
    points
}

fn collect_lines(value: &serde_json::Value, points: &mut Vec<[f64; 2]>) {
    if let Some(array) = value.as_array() {
        if let Some(point) = to_point(value) {
            points.push(point);
            return;
        }
        for item in array {
            collect_lines(item, points);
        }
    }
}

fn outer_ring(value: &serde_json::Value) -> Option<Vec<[f64; 2]>> {
    let ring = value.as_array()?.first()?.as_array()?;
    let points: Vec<[f64; 2]> = ring.iter().filter_map(to_point).collect();
    (!points.is_empty()).then_some(points)
}

fn parse_coordinates(text: &str) -> Vec<[f64; 2]> {
    text.split_whitespace()
        .filter_map(|tuple| {
            let mut parts = tuple.split(',');
            let lon = parts.next()?.trim().parse::<f64>().ok()?;
            let lat = parts.next()?.trim().parse::<f64>().ok()?;
            Some([lon, lat])
        })
        .collect()
}

fn parse_kml(text: &str) -> Result<GeoPreview, String> {
    let root = parse_document(text);
    let mut features = Vec::new();
    for placemark in root.descendants_named("Placemark") {
        if features.len() >= MAX_FEATURES {
            break;
        }
        let name = placemark
            .descendant("name")
            .map(|element| element.text_content().trim().to_string())
            .unwrap_or_default();
        let (kind, coordinates) = if let Some(point) = placemark.descendant("Point") {
            ("Point", point.descendant("coordinates"))
        } else if let Some(line) = placemark.descendant("LineString") {
            ("LineString", line.descendant("coordinates"))
        } else if let Some(polygon) = placemark.descendant("Polygon") {
            ("Polygon", polygon.descendant("coordinates"))
        } else {
            continue;
        };
        let Some(coordinates) = coordinates else {
            continue;
        };
        let points = parse_coordinates(&coordinates.text_content());
        if !points.is_empty() {
            features.push(GeoFeature {
                name,
                kind: kind.to_string(),
                points,
            });
        }
    }
    finalize("kml", features)
}

fn parse_gpx(text: &str) -> Result<GeoPreview, String> {
    let root = parse_document(text);
    let mut features = Vec::new();

    for waypoint in root.descendants_named("wpt") {
        if features.len() >= MAX_FEATURES {
            break;
        }
        let name = waypoint
            .descendant("name")
            .map(|element| element.text_content().trim().to_string())
            .unwrap_or_default();
        if let Some(point) = gpx_point(waypoint) {
            features.push(GeoFeature {
                name,
                kind: "Point".to_string(),
                points: vec![point],
            });
        }
    }

    for track in root.descendants_named("trk") {
        if features.len() >= MAX_FEATURES {
            break;
        }
        let name = track
            .descendant("name")
            .map(|element| element.text_content().trim().to_string())
            .unwrap_or_default();
        let points: Vec<[f64; 2]> = track
            .descendants_named("trkpt")
            .filter_map(gpx_point)
            .collect();
        if !points.is_empty() {
            features.push(GeoFeature {
                name,
                kind: "LineString".to_string(),
                points,
            });
        }
    }

    for route in root.descendants_named("rte") {
        if features.len() >= MAX_FEATURES {
            break;
        }
        let name = route
            .descendant("name")
            .map(|element| element.text_content().trim().to_string())
            .unwrap_or_default();
        let points: Vec<[f64; 2]> = route
            .descendants_named("rtept")
            .filter_map(gpx_point)
            .collect();
        if !points.is_empty() {
            features.push(GeoFeature {
                name,
                kind: "LineString".to_string(),
                points,
            });
        }
    }

    finalize("gpx", features)
}

fn gpx_point(element: &crate::preview::xml::Element) -> Option<[f64; 2]> {
    let lat = element.attr("lat")?.trim().parse::<f64>().ok()?;
    let lon = element.attr("lon")?.trim().parse::<f64>().ok()?;
    Some([lon, lat])
}

fn finalize(format: &str, features: Vec<GeoFeature>) -> Result<GeoPreview, String> {
    if features.is_empty() {
        return Err("No geometry found".to_string());
    }
    let mut min_lon = f64::MAX;
    let mut min_lat = f64::MAX;
    let mut max_lon = f64::MIN;
    let mut max_lat = f64::MIN;
    let mut point_count = 0;
    for feature in &features {
        for [lon, lat] in &feature.points {
            min_lon = min_lon.min(*lon);
            min_lat = min_lat.min(*lat);
            max_lon = max_lon.max(*lon);
            max_lat = max_lat.max(*lat);
            point_count += 1;
        }
    }
    Ok(GeoPreview {
        format: format.to_string(),
        features,
        bounds: Some([min_lon, min_lat, max_lon, max_lat]),
        point_count,
    })
}

pub(crate) fn parse_geo(text: &str, extension: &str) -> Result<GeoPreview, String> {
    match extension {
        "geojson" | "json" => parse_geojson(text),
        "kml" => parse_kml(text),
        "gpx" => parse_gpx(text),
        other => Err(format!("Unsupported geo format: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_geojson_points_and_lines() {
        let text = r#"{"type":"FeatureCollection","features":[
          {"type":"Feature","properties":{"name":"Bogotá"},"geometry":{"type":"Point","coordinates":[-74.07,4.71]}},
          {"type":"Feature","properties":{},"geometry":{"type":"LineString","coordinates":[[-74.07,4.71],[-74.05,4.72]]}}]}"#;
        let preview = parse_geo(text, "geojson").unwrap();
        assert_eq!(preview.features.len(), 2);
        assert_eq!(preview.features[0].kind, "Point");
        assert_eq!(preview.features[1].kind, "LineString");
        assert_eq!(preview.point_count, 3);
        assert_eq!(preview.bounds.unwrap()[0], -74.07);
    }

    #[test]
    fn parses_kml() {
        let text = r#"<kml xmlns="http://www.opengis.net/kml/2.2"><Document><Placemark>
          <name>Ruta</name><LineString><coordinates>-74.07,4.71,0 -74.05,4.72,0</coordinates></LineString>
          </Placemark></Document></kml>"#;
        let preview = parse_geo(text, "kml").unwrap();
        assert_eq!(preview.features[0].name, "Ruta");
        assert_eq!(preview.features[0].points.len(), 2);
    }

    #[test]
    fn parses_gpx() {
        let text = r#"<gpx xmlns="http://www.topografix.com/GPX/1/1"><wpt lat="4.71" lon="-74.07"><name>Inicio</name></wpt>
          <trk><name>Ruta</name><trkseg><trkpt lat="4.71" lon="-74.07"/><trkpt lat="4.72" lon="-74.05"/></trkseg></trk></gpx>"#;
        let preview = parse_geo(text, "gpx").unwrap();
        assert_eq!(preview.features.len(), 2);
        assert_eq!(preview.features[0].kind, "Point");
        assert_eq!(preview.features[1].points.len(), 2);
    }
}
