//! What a scan knows about itself besides its pixels (resolution, colour
//! profile, EXIF), so the crops cut from it can carry it along.
//!
//! Reading is best effort: a file with damaged or unusual metadata still
//! exports, just without that part.

use std::fs::File;
use std::io::{BufReader, Cursor, Read};
use std::path::Path;

use exif::experimental::Writer as ExifWriter;
use exif::{Field, In, Reader as ExifReader, Tag, Value};
use img_parts::jpeg::Jpeg;
use img_parts::png::Png;
use img_parts::{Bytes, ImageEXIF, ImageICC};

/// Resolution in dots per inch.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Dpi {
    pub x: f32,
    pub y: f32,
}

impl Dpi {
    /// Reject values that cannot be a real scan resolution.
    fn checked(x: f64, y: f64) -> Option<Dpi> {
        let ok = |v: f64| v.is_finite() && v > 0.0 && v <= 100_000.0;
        (ok(x) && ok(y)).then_some(Dpi {
            x: x as f32,
            y: y as f32,
        })
    }
}

/// Metadata of a scan, as found in the file.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Metadata {
    pub dpi: Option<Dpi>,
    /// Raw ICC profile.
    pub icc: Option<Vec<u8>>,
    /// Raw EXIF in TIFF structure (no `Exif\0\0` prefix), as in the source.
    pub exif: Option<Vec<u8>>,
    /// Write OxiCut into the crop's `Software` field. Set by the exporter
    /// from the user's choice; never read from a file.
    pub tag_software: bool,
}

/// What goes into the `Software` field of a crop when asked for.
pub fn software() -> String {
    format!("OxiCut {}", crate::VERSION)
}

impl Metadata {
    /// Nothing that was read from a file (the `tag_software` choice is not file content).
    pub fn is_empty(&self) -> bool {
        self.dpi.is_none() && self.icc.is_none() && self.exif.is_none()
    }

    /// The `Software` value to write, if the user wants OxiCut named.
    pub fn software_tag(&self) -> Option<String> {
        self.tag_software.then(software)
    }

    /// Read the metadata of the scan at `path`. The format is recognised from
    /// the file's first bytes, not its extension.
    pub fn read(path: &Path) -> Metadata {
        read_file(path).unwrap_or_default()
    }

    /// EXIF for a crop: the scan's own fields without what is wrong for a
    /// crop, the resolution if it is known, and `Software` set to OxiCut if
    /// asked for (otherwise the scan's own `Software` stays). `None` when
    /// there is nothing to write.
    pub fn crop_exif(&self) -> Option<Vec<u8>> {
        let software = self.software_tag();
        let software = software.as_deref();
        if let Some(raw) = &self.exif
            && let Some(rewritten) = rewrite_exif(raw, software, self.dpi, false)
        {
            // A JPEG APP1 segment holds 64 KB; maker notes are the usual cause of more.
            if rewritten.len() <= 60_000 {
                return Some(rewritten);
            }
            if let Some(smaller) = rewrite_exif(raw, software, self.dpi, true)
                && smaller.len() <= 60_000
            {
                return Some(smaller);
            }
        }
        rewrite_exif(&[], software, self.dpi, true)
    }
}

fn read_file(path: &Path) -> Option<Metadata> {
    let mut head = [0u8; 4];
    File::open(path).ok()?.read_exact(&mut head).ok()?;
    match head {
        [0xFF, 0xD8, ..] => read_jpeg(std::fs::read(path).ok()?),
        [0x89, b'P', b'N', b'G'] => read_png(std::fs::read(path).ok()?),
        [b'I', b'I', 42, 0] | [b'M', b'M', 0, 42] | [b'I', b'I', 43, 0] | [b'M', b'M', 0, 43] => {
            read_tiff(path)
        }
        _ => None,
    }
}

fn read_jpeg(data: Vec<u8>) -> Option<Metadata> {
    let jpeg = Jpeg::from_bytes(Bytes::from(data)).ok()?;
    let exif = jpeg.exif().map(|b| b.to_vec());
    let dpi = jfif_dpi(&jpeg).or_else(|| exif.as_deref().and_then(exif_dpi));
    Some(Metadata {
        dpi,
        icc: jpeg.icc_profile().map(|b| b.to_vec()),
        exif,
        ..Default::default()
    })
}

/// Density from the JFIF header (APP0): units 1 = dots per inch, 2 = per cm, 0 = aspect ratio only.
fn jfif_dpi(jpeg: &Jpeg) -> Option<Dpi> {
    let app0 = jpeg.segment_by_marker(0xE0)?;
    let c = app0.contents();
    if c.len() < 12 || &c[..5] != b"JFIF\0" {
        return None;
    }
    let (x, y) = (
        u16::from_be_bytes([c[8], c[9]]) as f64,
        u16::from_be_bytes([c[10], c[11]]) as f64,
    );
    match c[7] {
        1 => Dpi::checked(x, y),
        2 => Dpi::checked(x * 2.54, y * 2.54),
        _ => None,
    }
}

fn read_png(data: Vec<u8>) -> Option<Metadata> {
    let png = Png::from_bytes(Bytes::from(data)).ok()?;
    // pHYs: pixels per unit x, y (u32 each), unit 1 = metre.
    let dpi = png.chunk_by_type(*b"pHYs").and_then(|chunk| {
        let c = chunk.contents();
        if c.len() < 9 || c[8] != 1 {
            return None;
        }
        let per_metre = |i: usize| u32::from_be_bytes([c[i], c[i + 1], c[i + 2], c[i + 3]]) as f64;
        Dpi::checked(per_metre(0) * 0.0254, per_metre(4) * 0.0254)
    });
    Some(Metadata {
        dpi,
        icc: png.icc_profile().map(|b| b.to_vec()),
        exif: png.exif().map(|b| b.to_vec()),
        ..Default::default()
    })
}

/// TIFF: resolution and ICC profile of the first image. EXIF sub-IFDs are not read.
fn read_tiff(path: &Path) -> Option<Metadata> {
    use tiff::decoder::{Decoder, ifd::Value as TiffValue};
    use tiff::tags::Tag as TiffTag;

    let mut decoder = Decoder::new(BufReader::new(File::open(path).ok()?)).ok()?;
    let ratio = |value: Option<TiffValue>| match value? {
        TiffValue::Rational(n, d) if d != 0 => Some(n as f64 / d as f64),
        _ => None,
    };
    let x = ratio(decoder.find_tag(TiffTag::XResolution).ok()?);
    let y = ratio(decoder.find_tag(TiffTag::YResolution).ok()?);
    // ResolutionUnit: 1 none, 2 inch (the default when absent), 3 centimetre.
    let unit = decoder
        .find_tag_unsigned::<u16>(TiffTag::ResolutionUnit)
        .ok()
        .flatten()
        .unwrap_or(2);
    let per_inch = match unit {
        2 => 1.0,
        3 => 2.54,
        _ => 0.0,
    };
    let dpi = match (x, y) {
        (Some(x), Some(y)) => Dpi::checked(x * per_inch, y * per_inch),
        _ => None,
    };
    let icc = decoder
        .find_tag_unsigned_vec::<u8>(TiffTag::IccProfile)
        .ok()
        .flatten();
    Some(Metadata {
        dpi,
        icc,
        exif: None,
        ..Default::default()
    })
}

/// Resolution from an EXIF block (X/YResolution with ResolutionUnit).
fn exif_dpi(raw: &[u8]) -> Option<Dpi> {
    let exif = ExifReader::new().read_raw(raw.to_vec()).ok()?;
    let rational = |tag| match &exif.get_field(tag, In::PRIMARY)?.value {
        Value::Rational(v) => v.first().map(|r| r.to_f64()),
        _ => None,
    };
    let per_inch = match exif
        .get_field(Tag::ResolutionUnit, In::PRIMARY)
        .and_then(|f| f.value.get_uint(0))
    {
        Some(3) => 2.54,
        Some(2) | None => 1.0,
        _ => return None,
    };
    Dpi::checked(
        rational(Tag::XResolution)? * per_inch,
        rational(Tag::YResolution)? * per_inch,
    )
}

/// Tags that describe the scan's pixels or the scan's own thumbnail and are
/// wrong for a crop.
const NOT_FOR_CROPS: [Tag; 7] = [
    Tag::PixelXDimension,
    Tag::PixelYDimension,
    Tag::ImageWidth,
    Tag::ImageLength,
    Tag::XResolution,
    Tag::YResolution,
    Tag::ResolutionUnit,
];

/// Rebuild an EXIF block for a crop. Fields of the main image are kept except
/// [`NOT_FOR_CROPS`] (the thumbnail IFD goes entirely: it shows the whole scan);
/// `Software` is set when given (else the scan's own stays) and so is the resolution.
/// `raw` may be empty. `None` when there is nothing to write.
fn rewrite_exif(
    raw: &[u8],
    software: Option<&str>,
    dpi: Option<Dpi>,
    drop_maker_note: bool,
) -> Option<Vec<u8>> {
    let parsed = if raw.is_empty() {
        None
    } else {
        ExifReader::new().read_raw(raw.to_vec()).ok()
    };
    let mut fields: Vec<Field> = parsed
        .iter()
        .flat_map(|exif| exif.fields())
        .filter(|f| f.ifd_num == In::PRIMARY)
        .filter(|f| !NOT_FOR_CROPS.contains(&f.tag))
        .filter(|f| !(software.is_some() && f.tag == Tag::Software))
        .filter(|f| !(drop_maker_note && f.tag == Tag::MakerNote))
        .cloned()
        .collect();
    let new = |tag, value| Field {
        tag,
        ifd_num: In::PRIMARY,
        value,
    };
    if let Some(software) = software {
        fields.push(new(Tag::Software, Value::Ascii(vec![software.as_bytes().to_vec()])));
    }
    if let Some(dpi) = dpi {
        let ratio = |v: f32| exif::Rational {
            num: (v * 100.0).round() as u32,
            denom: 100,
        };
        fields.push(new(Tag::XResolution, Value::Rational(vec![ratio(dpi.x)])));
        fields.push(new(Tag::YResolution, Value::Rational(vec![ratio(dpi.y)])));
        fields.push(new(Tag::ResolutionUnit, Value::Short(vec![2])));
    }

    if fields.is_empty() {
        return None;
    }
    let mut writer = ExifWriter::new();
    for field in &fields {
        writer.push_field(field);
    }
    let mut out = Cursor::new(Vec::new());
    let little_endian = parsed.as_ref().is_none_or(|e| e.little_endian());
    writer.write(&mut out, little_endian).ok()?;
    Some(out.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exif_with(fields: &[Field]) -> Vec<u8> {
        let mut writer = ExifWriter::new();
        for f in fields {
            writer.push_field(f);
        }
        let mut out = Cursor::new(Vec::new());
        writer.write(&mut out, true).unwrap();
        out.into_inner()
    }

    fn field(tag: Tag, ifd: In, value: Value) -> Field {
        Field {
            tag,
            ifd_num: ifd,
            value,
        }
    }

    fn ascii(text: &str) -> Value {
        Value::Ascii(vec![text.as_bytes().to_vec()])
    }

    #[test]
    fn the_crop_says_oxicut_and_loses_the_scans_size_and_thumbnail() {
        let scan = exif_with(&[
            field(Tag::Make, In::PRIMARY, ascii("EPSON")),
            field(Tag::Software, In::PRIMARY, ascii("EPSON Scan")),
            field(Tag::DateTimeOriginal, In::PRIMARY, ascii("2024:05:01 10:00:00")),
            field(Tag::PixelXDimension, In::PRIMARY, Value::Long(vec![5000])),
            field(Tag::Orientation, In::PRIMARY, Value::Short(vec![1])),
            field(Tag::Make, In::THUMBNAIL, ascii("thumbnail only")),
        ]);
        let meta = Metadata {
            dpi: Some(Dpi { x: 600.0, y: 600.0 }),
            icc: None,
            exif: Some(scan),
            tag_software: true,
        };

        let crop = ExifReader::new().read_raw(meta.crop_exif().unwrap()).unwrap();
        let text = |tag| match &crop.get_field(tag, In::PRIMARY).unwrap().value {
            Value::Ascii(v) => String::from_utf8(v[0].clone()).unwrap(),
            other => panic!("{other:?}"),
        };
        assert_eq!(text(Tag::Software), software());
        assert_eq!(text(Tag::Make), "EPSON");
        assert_eq!(text(Tag::DateTimeOriginal), "2024:05:01 10:00:00");
        assert!(crop.get_field(Tag::PixelXDimension, In::PRIMARY).is_none());
        assert!(crop.fields().all(|f| f.ifd_num == In::PRIMARY));
        assert_eq!(exif_dpi(&meta.crop_exif().unwrap()), Some(Dpi { x: 600.0, y: 600.0 }));
    }

    #[test]
    fn a_scan_without_exif_still_gets_the_software_field() {
        let crop = ExifReader::new()
            .read_raw(
                Metadata {
                    tag_software: true,
                    ..Default::default()
                }
                .crop_exif()
                .unwrap(),
            )
            .unwrap();
        assert!(crop.get_field(Tag::Software, In::PRIMARY).is_some());
        assert!(crop.get_field(Tag::XResolution, In::PRIMARY).is_none());
    }

    #[test]
    fn centimetre_resolution_is_converted_and_nonsense_is_ignored() {
        let cm = exif_with(&[
            field(
                Tag::XResolution,
                In::PRIMARY,
                Value::Rational(vec![exif::Rational { num: 236, denom: 1 }]),
            ),
            field(
                Tag::YResolution,
                In::PRIMARY,
                Value::Rational(vec![exif::Rational { num: 236, denom: 1 }]),
            ),
            field(Tag::ResolutionUnit, In::PRIMARY, Value::Short(vec![3])),
        ]);
        let dpi = exif_dpi(&cm).unwrap();
        assert!((dpi.x - 599.4).abs() < 0.1, "{dpi:?}");
        assert!(Dpi::checked(0.0, 300.0).is_none());
        assert!(Dpi::checked(f64::NAN, 300.0).is_none());
        assert!(Dpi::checked(300.0, 1e9).is_none());
    }

    #[test]
    fn unreadable_files_have_no_metadata() {
        assert!(Metadata::read(Path::new("does-not-exist.jpg")).is_empty());
    }
}
