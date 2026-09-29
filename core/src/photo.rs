//! The photo store (spec §16): files are copied under the database's `photos/` directory,
//! named by content hash, so a photo outlives wherever it was first saved.

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, ImageDecoder, ImageReader};
use sha2::{Digest, Sha256};

use crate::{Error, Result};

/// A crop in fractions of the upright photo: left, top, width, height, each 0–1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Crop {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl FromStr for Crop {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self> {
        let parts: Vec<f64> = s
            .split(',')
            .map(|p| p.trim().parse::<f64>())
            .collect::<std::result::Result<_, _>>()
            .map_err(|_| Error::Usage(format!("crop `{s}` is not four numbers x,y,w,h")))?;
        let [x, y, w, h] = parts[..] else {
            return Err(Error::Usage(format!(
                "crop `{s}` needs exactly four numbers x,y,w,h"
            )));
        };
        let ok = |v: f64| (0.0..=1.0).contains(&v);
        if !(ok(x) && ok(y) && w > 0.0 && h > 0.0 && x + w <= 1.0001 && y + h <= 1.0001) {
            return Err(Error::Usage(format!(
                "crop `{s}` must lie inside the photo: fractions 0–1 with x+w ≤ 1 and y+h ≤ 1"
            )));
        }
        Ok(Crop { x, y, w, h })
    }
}

impl std::fmt::Display for Crop {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:.4},{:.4},{:.4},{:.4}", self.x, self.y, self.w, self.h)
    }
}

fn io(path: &Path) -> impl Fn(std::io::Error) -> Error + '_ {
    move |e| Error::Usage(format!("{}: {e}", path.display()))
}

/// Copies bytes into the store under their hash; returns the stored path.
pub(crate) fn store_bytes(dir: &Path, bytes: &[u8], ext: &str) -> Result<PathBuf> {
    std::fs::create_dir_all(dir).map_err(io(dir))?;
    let hash = Sha256::digest(bytes);
    let name: String = hash.iter().take(10).map(|b| format!("{b:02x}")).collect();
    let target = dir.join(format!("{name}.{ext}"));
    if !target.exists() {
        std::fs::write(&target, bytes).map_err(io(&target))?;
    }
    Ok(target)
}

fn extension(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .filter(|e| ["jpg", "jpeg", "png", "webp", "heic"].contains(&e.as_str()))
        .map(|e| if e == "jpeg" { "jpg".to_string() } else { e })
        .unwrap_or_else(|| "jpg".to_string())
}

pub(crate) fn store_file(dir: &Path, file: &Path) -> Result<PathBuf> {
    let bytes = std::fs::read(file).map_err(io(file))?;
    store_bytes(dir, &bytes, &extension(file))
}

/// Decodes a photo the right way up, honouring its EXIF orientation.
pub fn open_upright(file: &Path) -> Result<DynamicImage> {
    let bad = |e: image::ImageError| Error::Usage(format!("{}: {e}", file.display()));
    let mut decoder = ImageReader::open(file)
        .map_err(io(file))?
        .with_guessed_format()
        .map_err(io(file))?
        .into_decoder()
        .map_err(bad)?;
    let orientation = decoder.orientation().map_err(bad)?;
    let mut img = DynamicImage::from_decoder(decoder).map_err(bad)?;
    img.apply_orientation(orientation);
    Ok(img)
}

/// Cuts `crop` out of `file` and stores it as a JPEG.
pub(crate) fn store_crop(dir: &Path, file: &Path, crop: Crop) -> Result<PathBuf> {
    let img = open_upright(file)?;
    let (w, h) = (f64::from(img.width()), f64::from(img.height()));
    let px = |v: f64, max: f64| (v * max).round().clamp(0.0, max) as u32;
    let (x, y) = (px(crop.x, w), px(crop.y, h));
    let cw = px(crop.w, w).min(img.width() - x).max(1);
    let ch = px(crop.h, h).min(img.height() - y).max(1);
    let cut = img.crop_imm(x, y, cw, ch).to_rgb8();
    let mut buf = Cursor::new(Vec::new());
    cut.write_with_encoder(JpegEncoder::new_with_quality(&mut buf, 88))
        .map_err(|e| Error::Internal(format!("encoding crop: {e}")))?;
    store_bytes(dir, buf.get_ref(), "jpg")
}

/// Where a mark goes on a photo, in fractions of the upright photo: a rectangle, or the four
/// corners of a grid's cells (back-left, back-right, front-right, front-left), which perspective
/// leaves a quadrilateral.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Shape {
    Rect(Crop),
    Quad([(f64, f64); 4]),
}

const MARK_RED: image::Rgb<u8> = image::Rgb([230, 30, 30]);
const MARK_WHITE: image::Rgb<u8> = image::Rgb([255, 255, 255]);

/// A 5×7 bitmap of a label character, one row per byte, bit 4 the leftmost column. Labels are
/// short (a number, a cell, an arrow), so a built-in font keeps marking free of font files.
/// Lowercase reads as uppercase and Turkish letters as their plain forms; anything else is `?`.
fn glyph(c: char) -> [u8; 7] {
    let c = match c {
        'ç' | 'Ç' => 'C',
        'ğ' | 'Ğ' => 'G',
        'ı' | 'İ' => 'I',
        'ö' | 'Ö' => 'O',
        'ş' | 'Ş' => 'S',
        'ü' | 'Ü' => 'U',
        c => c.to_ascii_uppercase(),
    };
    match c {
        '0' => [0x0e, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0e],
        '1' => [0x04, 0x0c, 0x04, 0x04, 0x04, 0x04, 0x0e],
        '2' => [0x0e, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1f],
        '3' => [0x1f, 0x02, 0x04, 0x02, 0x01, 0x11, 0x0e],
        '4' => [0x02, 0x06, 0x0a, 0x12, 0x1f, 0x02, 0x02],
        '5' => [0x1f, 0x10, 0x1e, 0x01, 0x01, 0x11, 0x0e],
        '6' => [0x06, 0x08, 0x10, 0x1e, 0x11, 0x11, 0x0e],
        '7' => [0x1f, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
        '8' => [0x0e, 0x11, 0x11, 0x0e, 0x11, 0x11, 0x0e],
        '9' => [0x0e, 0x11, 0x11, 0x0f, 0x01, 0x02, 0x0c],
        'A' => [0x0e, 0x11, 0x11, 0x1f, 0x11, 0x11, 0x11],
        'B' => [0x1e, 0x11, 0x11, 0x1e, 0x11, 0x11, 0x1e],
        'C' => [0x0e, 0x11, 0x10, 0x10, 0x10, 0x11, 0x0e],
        'D' => [0x1c, 0x12, 0x11, 0x11, 0x11, 0x12, 0x1c],
        'E' => [0x1f, 0x10, 0x10, 0x1e, 0x10, 0x10, 0x1f],
        'F' => [0x1f, 0x10, 0x10, 0x1e, 0x10, 0x10, 0x10],
        'G' => [0x0e, 0x11, 0x10, 0x17, 0x11, 0x11, 0x0f],
        'H' => [0x11, 0x11, 0x11, 0x1f, 0x11, 0x11, 0x11],
        'I' => [0x0e, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0e],
        'J' => [0x07, 0x02, 0x02, 0x02, 0x02, 0x12, 0x0c],
        'K' => [0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11],
        'L' => [0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1f],
        'M' => [0x11, 0x1b, 0x15, 0x15, 0x11, 0x11, 0x11],
        'N' => [0x11, 0x11, 0x19, 0x15, 0x13, 0x11, 0x11],
        'O' => [0x0e, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0e],
        'P' => [0x1e, 0x11, 0x11, 0x1e, 0x10, 0x10, 0x10],
        'Q' => [0x0e, 0x11, 0x11, 0x11, 0x15, 0x12, 0x0d],
        'R' => [0x1e, 0x11, 0x11, 0x1e, 0x14, 0x12, 0x11],
        'S' => [0x0f, 0x10, 0x10, 0x0e, 0x01, 0x01, 0x1e],
        'T' => [0x1f, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04],
        'U' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0e],
        'V' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x0a, 0x04],
        'W' => [0x11, 0x11, 0x11, 0x15, 0x15, 0x15, 0x0a],
        'X' => [0x11, 0x11, 0x0a, 0x04, 0x0a, 0x11, 0x11],
        'Y' => [0x11, 0x11, 0x11, 0x0a, 0x04, 0x04, 0x04],
        'Z' => [0x1f, 0x01, 0x02, 0x04, 0x08, 0x10, 0x1f],
        ' ' => [0; 7],
        '+' => [0x00, 0x04, 0x04, 0x1f, 0x04, 0x04, 0x00],
        '-' => [0x00, 0x00, 0x00, 0x1f, 0x00, 0x00, 0x00],
        '>' => [0x08, 0x04, 0x02, 0x01, 0x02, 0x04, 0x08],
        '→' => [0x00, 0x04, 0x02, 0x1f, 0x02, 0x04, 0x00],
        '.' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x0c, 0x0c],
        ',' => [0x00, 0x00, 0x00, 0x00, 0x0c, 0x04, 0x08],
        ':' => [0x00, 0x0c, 0x0c, 0x00, 0x0c, 0x0c, 0x00],
        '/' => [0x01, 0x01, 0x02, 0x04, 0x08, 0x10, 0x10],
        '(' => [0x02, 0x04, 0x08, 0x08, 0x08, 0x04, 0x02],
        ')' => [0x08, 0x04, 0x02, 0x02, 0x02, 0x04, 0x08],
        '#' => [0x0a, 0x0a, 0x1f, 0x0a, 0x1f, 0x0a, 0x0a],
        '×' => [0x00, 0x11, 0x0a, 0x04, 0x0a, 0x11, 0x00],
        _ => [0x0e, 0x11, 0x01, 0x02, 0x04, 0x00, 0x04],
    }
}

/// A filled square of side `t` centred on `(x, y)`, clipped to the image.
fn dot(img: &mut image::RgbImage, x: f64, y: f64, t: f64, color: image::Rgb<u8>) {
    let half = t / 2.0;
    let (w, h) = (img.width() as f64, img.height() as f64);
    let (x0, x1) = ((x - half).max(0.0), (x + half).min(w));
    let (y0, y1) = ((y - half).max(0.0), (y + half).min(h));
    for py in y0 as u32..y1 as u32 {
        for px in x0 as u32..x1 as u32 {
            img.put_pixel(px, py, color);
        }
    }
}

fn line(img: &mut image::RgbImage, a: (f64, f64), b: (f64, f64), t: f64) {
    let steps = (b.0 - a.0).abs().max((b.1 - a.1).abs()).ceil().max(1.0);
    for i in 0..=steps as u32 {
        let k = f64::from(i) / steps;
        dot(
            img,
            a.0 + (b.0 - a.0) * k,
            a.1 + (b.1 - a.1) * k,
            t,
            MARK_RED,
        );
    }
}

/// A label's size in pixels at scale `s`: 6 columns per character (one of them space), 7 rows,
/// and a red margin of `2s` around.
fn label_size(text: &str, s: f64) -> (f64, f64) {
    let n = text.chars().count() as f64;
    ((n * 6.0 - 1.0) * s + 4.0 * s, 7.0 * s + 4.0 * s)
}

/// White text on a red plate, its top-left corner at `(x, y)`.
fn label(img: &mut image::RgbImage, text: &str, x: f64, y: f64, s: f64) {
    let (w, h) = label_size(text, s);
    let (iw, ih) = (img.width() as f64, img.height() as f64);
    let (x, y) = (
        x.clamp(0.0, (iw - w).max(0.0)),
        y.clamp(0.0, (ih - h).max(0.0)),
    );
    for py in y as u32..((y + h).min(ih)) as u32 {
        for px in x as u32..((x + w).min(iw)) as u32 {
            img.put_pixel(px, py, MARK_RED);
        }
    }
    for (i, c) in text.chars().enumerate() {
        let g = glyph(c);
        let gx = x + 2.0 * s + i as f64 * 6.0 * s;
        for (row, bits) in g.iter().enumerate() {
            for col in 0..5 {
                if bits & (0x10 >> col) != 0 {
                    let (cx, cy) = (gx + col as f64 * s, y + 2.0 * s + row as f64 * s);
                    dot(img, cx + s / 2.0, cy + s / 2.0, s, MARK_WHITE);
                }
            }
        }
    }
}

/// Draws each `(label, shape)` on a copy of `file` and writes it to `out` as a JPEG: a red frame
/// around the shape, and the label on a red plate — above a rectangle's top-left corner (inside
/// it at the photo's top edge), in the middle of a grid's cells. The original is not touched.
pub(crate) fn draw_marks(file: &Path, marks: &[(String, Shape)], out: &Path) -> Result<()> {
    let mut img = open_upright(file)?.to_rgb8();
    let (w, h) = (f64::from(img.width()), f64::from(img.height()));
    let short = w.min(h);
    let t = (short / 180.0).max(3.0);
    let s = (short / 150.0).max(2.0).round();
    let mut placed: Vec<(f64, f64, f64, f64)> = Vec::new();
    for (text, shape) in marks {
        let pts: [(f64, f64); 4] = match shape {
            Shape::Rect(c) => [
                (c.x, c.y),
                (c.x + c.w, c.y),
                (c.x + c.w, c.y + c.h),
                (c.x, c.y + c.h),
            ],
            Shape::Quad(q) => *q,
        };
        let px: Vec<(f64, f64)> = pts.iter().map(|(x, y)| (x * w, y * h)).collect();
        for i in 0..4 {
            line(&mut img, px[i], px[(i + 1) % 4], t);
        }
        let (lw, lh) = label_size(text, s);
        let (x, y) = match shape {
            Shape::Rect(_) => {
                // Above the frame, else below it, else inside its top: the first place that
                // stays in the photo and clear of the labels already drawn.
                let bottom = px[2].1;
                let tries = [
                    (px[0].0 - t / 2.0, px[0].1 - lh - t),
                    (px[0].0 - t / 2.0, bottom + t),
                    (px[0].0 + t, px[0].1 + t),
                ];
                let fits = |&(x, y): &(f64, f64)| {
                    let inside = y >= 0.0 && y + lh <= h && x + lw <= w;
                    let clear = placed
                        .iter()
                        .all(|&(px, py, pw, ph): &(f64, f64, f64, f64)| {
                            x + lw <= px || px + pw <= x || y + lh <= py || py + ph <= y
                        });
                    inside && clear
                };
                tries.iter().copied().find(fits).unwrap_or(tries[0])
            }
            Shape::Quad(_) => {
                let cx = px.iter().map(|p| p.0).sum::<f64>() / 4.0;
                let cy = px.iter().map(|p| p.1).sum::<f64>() / 4.0;
                (cx - lw / 2.0, cy - lh / 2.0)
            }
        };
        placed.push((x, y, lw, lh));
        label(&mut img, text, x, y, s);
    }
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir).map_err(io(dir))?;
    }
    let mut buf = Cursor::new(Vec::new());
    img.write_with_encoder(JpegEncoder::new_with_quality(&mut buf, 88))
        .map_err(|e| Error::Internal(format!("encoding marked photo: {e}")))?;
    std::fs::write(out, buf.get_ref()).map_err(io(out))
}

#[cfg(test)]
mod tests {
    use super::Crop;

    #[test]
    fn crops_parse_and_must_fit() {
        assert!("0.1,0.2,0.3,0.4".parse::<Crop>().is_ok());
        assert!("0.5,0,0.6,1".parse::<Crop>().is_err());
        assert!("0,0,0,1".parse::<Crop>().is_err());
        assert!("a,b,c,d".parse::<Crop>().is_err());
        assert!("0,0,1".parse::<Crop>().is_err());
    }
}
