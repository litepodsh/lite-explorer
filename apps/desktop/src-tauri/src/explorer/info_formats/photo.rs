//! Readable summary of the EXIF fields people look for; the full tag list stays in `explorer::exif`.
use super::{one_decimal, InfoSection};
use exif::{Exif, Field, In, Reader, Tag, Value};
use std::{fs::File, io::BufReader, path::Path};

const MAX_BYTES: u64 = 256 * 1024 * 1024;

pub(crate) fn section(path: &Path, length: u64) -> Option<InfoSection> {
    if length > MAX_BYTES {
        return None;
    }
    let mut reader = BufReader::new(File::open(path).ok()?);
    summarize(&Reader::new().read_from_container(&mut reader).ok()?)
}

fn field(data: &Exif, tag: Tag) -> Option<&Field> {
    data.get_field(tag, In::PRIMARY)
}

fn text(data: &Exif, tag: Tag) -> Option<String> {
    match &field(data, tag)?.value {
        Value::Ascii(parts) => parts
            .first()
            .map(|bytes| {
                String::from_utf8_lossy(bytes)
                    .trim_matches(|c: char| c == '\0' || c.is_whitespace())
                    .to_string()
            })
            .filter(|value| !value.is_empty()),
        _ => None,
    }
}

/// Every rational of the field, or `None` when any has a zero denominator.
fn rationals(data: &Exif, tag: Tag) -> Option<Vec<f64>> {
    match &field(data, tag)?.value {
        Value::Rational(values) => values
            .iter()
            .map(|value| (value.denom != 0).then(|| value.to_f64()))
            .collect(),
        _ => None,
    }
}

fn first_rational(data: &Exif, tag: Tag) -> Option<f64> {
    rationals(data, tag)?.first().copied()
}

fn uint(data: &Exif, tag: Tag) -> Option<u32> {
    field(data, tag)?.value.get_uint(0)
}

fn summarize(data: &Exif) -> Option<InfoSection> {
    let mut section = InfoSection::new("Photo");
    section.push_opt(
        "Camera",
        camera(text(data, Tag::Make), text(data, Tag::Model)),
    );
    section.push_opt("Lens", text(data, Tag::LensModel));
    let exposure: Vec<String> = [
        first_rational(data, Tag::FNumber)
            .filter(|f| *f > 0.0)
            .map(|f| format!("f/{}", one_decimal(f))),
        first_rational(data, Tag::ExposureTime)
            .filter(|t| *t > 0.0)
            .map(shutter),
        uint(data, Tag::PhotographicSensitivity).map(|iso| format!("ISO {iso}")),
    ]
    .into_iter()
    .flatten()
    .collect();
    if !exposure.is_empty() {
        section.push("Exposure", exposure.join(" · "));
    }
    if let Some(focal) = first_rational(data, Tag::FocalLength).filter(|f| *f > 0.0) {
        let equivalent = uint(data, Tag::FocalLengthIn35mmFilm)
            .filter(|mm| *mm > 0)
            .map(|mm| format!(" ({mm} mm equiv.)"))
            .unwrap_or_default();
        section.push(
            "Focal length",
            format!("{} mm{equivalent}", one_decimal(focal)),
        );
    }
    section.push_opt(
        "Taken",
        text(data, Tag::DateTimeOriginal).map(|taken| {
            let taken = exif_date(&taken);
            match text(data, Tag::OffsetTimeOriginal) {
                Some(offset) => format!("{taken} {offset}"),
                None => taken,
            }
        }),
    );
    if let (Some(latitude), Some(longitude)) = (
        coordinate(data, Tag::GPSLatitude, Tag::GPSLatitudeRef),
        coordinate(data, Tag::GPSLongitude, Tag::GPSLongitudeRef),
    ) {
        section.push("Location", format!("{latitude}, {longitude}"));
    }
    if let Some(altitude) = first_rational(data, Tag::GPSAltitude) {
        let below_sea_level = uint(data, Tag::GPSAltitudeRef) == Some(1);
        section.push(
            "Altitude",
            format!(
                "{:.0} m",
                if below_sea_level { -altitude } else { altitude }
            ),
        );
    }
    section.push_opt(
        "Flash",
        uint(data, Tag::Flash).map(|flash| {
            if flash & 1 == 1 {
                "Fired"
            } else {
                "Did not fire"
            }
        }),
    );
    section.push_opt(
        "Orientation",
        field(data, Tag::Orientation).map(|field| field.display_value().to_string()),
    );
    section.push_opt("Software", text(data, Tag::Software));
    section.non_empty()
}

/// `Canon` + `Canon EOS R6` → `Canon EOS R6`; `NIKON CORPORATION` + `NIKON D750` → `NIKON D750`.
fn camera(make: Option<String>, model: Option<String>) -> Option<String> {
    match (make, model) {
        (Some(make), Some(model)) => {
            let brand = make
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .to_lowercase();
            Some(
                if !brand.is_empty() && model.to_lowercase().starts_with(&brand) {
                    model
                } else {
                    format!("{make} {model}")
                },
            )
        }
        (make, model) => make.or(model),
    }
}

fn shutter(seconds: f64) -> String {
    if seconds < 1.0 {
        format!("1/{} s", (1.0 / seconds).round())
    } else {
        format!("{} s", one_decimal(seconds))
    }
}

/// `2024:03:01 14:22:05` → `2024-03-01 14:22`.
fn exif_date(value: &str) -> String {
    match chrono::NaiveDateTime::parse_from_str(value, "%Y:%m:%d %H:%M:%S") {
        Ok(date) => date.format("%Y-%m-%d %H:%M").to_string(),
        Err(_) => value.to_string(),
    }
}

fn coordinate(data: &Exif, tag: Tag, reference: Tag) -> Option<String> {
    let parts = rationals(data, tag)?;
    let [degrees, minutes, seconds] = parts.as_slice() else {
        return None;
    };
    let hemisphere = text(data, reference)?;
    Some(format!(
        "{:.4}° {hemisphere}",
        degrees + minutes / 60.0 + seconds / 3600.0
    ))
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{rows, rows_for};
    use exif::{experimental::Writer, Field, In, Rational, Tag, Value};
    use std::io::Cursor;

    fn ascii(tag: Tag, value: &str) -> Field {
        Field {
            tag,
            ifd_num: In::PRIMARY,
            value: Value::Ascii(vec![value.as_bytes().to_vec()]),
        }
    }
    fn rational(tag: Tag, parts: &[(u32, u32)]) -> Field {
        Field {
            tag,
            ifd_num: In::PRIMARY,
            value: Value::Rational(
                parts
                    .iter()
                    .map(|&(num, denom)| Rational { num, denom })
                    .collect(),
            ),
        }
    }
    fn short(tag: Tag, value: u16) -> Field {
        Field {
            tag,
            ifd_num: In::PRIMARY,
            value: Value::Short(vec![value]),
        }
    }

    /// A JPEG holding only an APP1 EXIF segment with `fields`.
    fn jpeg_with(fields: &[Field]) -> Vec<u8> {
        let mut writer = Writer::new();
        for field in fields {
            writer.push_field(field);
        }
        let mut tiff = Cursor::new(Vec::new());
        writer.write(&mut tiff, false).unwrap();
        let tiff = tiff.into_inner();
        let mut out = vec![0xff, 0xd8, 0xff, 0xe1];
        out.extend(((tiff.len() + 8) as u16).to_be_bytes());
        out.extend(b"Exif\0\0");
        out.extend(tiff);
        out.extend([0xff, 0xd9]);
        out
    }

    #[test]
    fn summarizes_camera_exposure_and_location() {
        let fields = [
            ascii(Tag::Make, "Canon"),
            ascii(Tag::Model, "Canon EOS R6"),
            ascii(Tag::LensModel, "RF50mm F1.8 STM"),
            rational(Tag::FNumber, &[(18, 10)]),
            rational(Tag::ExposureTime, &[(1, 120)]),
            short(Tag::PhotographicSensitivity, 100),
            rational(Tag::FocalLength, &[(50, 1)]),
            short(Tag::FocalLengthIn35mmFilm, 75),
            ascii(Tag::DateTimeOriginal, "2024:03:01 14:22:05"),
            ascii(Tag::OffsetTimeOriginal, "+02:00"),
            ascii(Tag::GPSLatitudeRef, "N"),
            rational(Tag::GPSLatitude, &[(40, 1), (25, 1), (0, 1)]),
            ascii(Tag::GPSLongitudeRef, "W"),
            rational(Tag::GPSLongitude, &[(3, 1), (42, 1), (13, 1)]),
            Field {
                tag: Tag::GPSAltitudeRef,
                ifd_num: In::PRIMARY,
                value: Value::Byte(vec![0]),
            },
            rational(Tag::GPSAltitude, &[(657, 1)]),
            short(Tag::Flash, 16),
        ];
        assert_eq!(
            rows_for(&jpeg_with(&fields), "photo.jpg", "Photo"),
            rows(&[
                ("Camera", "Canon EOS R6"),
                ("Lens", "RF50mm F1.8 STM"),
                ("Exposure", "f/1.8 · 1/120 s · ISO 100"),
                ("Focal length", "50 mm (75 mm equiv.)"),
                ("Taken", "2024-03-01 14:22 +02:00"),
                ("Location", "40.4167° N, 3.7036° W"),
                ("Altitude", "657 m"),
                ("Flash", "Did not fire"),
            ])
        );
    }

    #[test]
    fn zero_denominator_rationals_are_skipped() {
        let fields = [
            rational(Tag::FNumber, &[(18, 0)]),
            short(Tag::PhotographicSensitivity, 400),
        ];
        assert_eq!(
            rows_for(&jpeg_with(&fields), "cheap.jpg", "Photo"),
            rows(&[("Exposure", "ISO 400")])
        );
    }

    #[test]
    fn no_exif_means_no_section() {
        assert!(rows_for(&[0xff, 0xd8, 0xff, 0xd9], "bare.jpg", "Photo").is_empty());
        assert!(rows_for(
            &jpeg_with(&[ascii(Tag::Artist, "Ana")]),
            "artist.jpg",
            "Photo"
        )
        .is_empty());
    }
}
