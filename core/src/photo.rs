//! The photo store (spec §16): files are copied under the database's `photos/` directory,
//! named by content hash, so a photo outlives wherever it was first saved.

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, ImageDecoder, ImageReader};
use sha2::{Digest, Sha256};

use serde_json::json;

use crate::error::usage;
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
            .map_err(|_| usage("photo_crop_not_numbers", json!({ "crop": s })))?;
        let [x, y, w, h] = parts[..] else {
            return Err(usage("photo_crop_not_four", json!({ "crop": s })));
        };
        let ok = |v: f64| (0.0..=1.0).contains(&v);
        if !(ok(x) && ok(y) && w > 0.0 && h > 0.0 && x + w <= 1.0001 && y + h <= 1.0001) {
            return Err(usage("photo_crop_outside", json!({ "crop": s })));
        }
        Ok(Crop { x, y, w, h })
    }
}

impl std::fmt::Display for Crop {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:.4},{:.4},{:.4},{:.4}", self.x, self.y, self.w, self.h)
    }
}

impl Crop {
    /// The crop grown on every side by `pad` of its own width and height, inside the photo: a
    /// part whose edge an estimate cut off (a disc's rim, a bit's shank) stays whole.
    pub fn padded(self, pad: f64) -> Crop {
        let (dx, dy) = (self.w * pad, self.h * pad);
        let x = (self.x - dx).max(0.0);
        let y = (self.y - dy).max(0.0);
        Crop {
            x,
            y,
            w: (self.x + self.w + dx).min(1.0) - x,
            h: (self.y + self.h + dy).min(1.0) - y,
        }
    }

    /// The same part of the photo after `turns` clockwise quarter turns (spec/rotate.md).
    pub(crate) fn turned(self, turns: u8) -> Crop {
        let mut c = self;
        for _ in 0..turns {
            c = Crop {
                x: (1.0 - c.y - c.h).max(0.0),
                y: c.x,
                w: c.h,
                h: c.w,
            };
        }
        c
    }
}

/// Clockwise quarter turns for a turn in degrees: 90, 180 or 270.
pub fn quarter_turns(degrees: u16) -> Result<u8> {
    match degrees {
        90 => Ok(1),
        180 => Ok(2),
        270 => Ok(3),
        _ => Err(usage("photo_turn_bad", json!({ "degrees": degrees }))),
    }
}

/// A point of the photo, in fractions, after `turns` clockwise quarter turns.
pub(crate) fn turn_point((x, y): (f64, f64), turns: u8) -> (f64, f64) {
    let mut p = (x, y);
    for _ in 0..turns {
        p = (1.0 - p.1, p.0);
    }
    p
}

/// The photo turned `turns` clockwise quarter turns, upright first, as a JPEG.
pub(crate) fn turned_jpeg(file: &Path, turns: u8) -> Result<Vec<u8>> {
    let img = open_upright(file)?;
    let img = match turns {
        1 => img.rotate90(),
        2 => img.rotate180(),
        3 => img.rotate270(),
        _ => img,
    };
    let mut buf = Cursor::new(Vec::new());
    img.to_rgb8()
        .write_with_encoder(JpegEncoder::new_with_quality(&mut buf, 92))
        .map_err(|e| Error::Internal(format!("encoding a turned photo: {e}")))?;
    Ok(buf.into_inner())
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

/// The shorter side of an image in pixels, read from its header; `None` for a file that is not
/// an image ev can read (a PDF).
pub(crate) fn short_side(file: &Path) -> Option<u32> {
    image::image_dimensions(file).ok().map(|(w, h)| w.min(h))
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
/// The edge around every red stroke and plate.
const MARK_DARK: image::Rgb<u8> = image::Rgb([20, 20, 20]);

/// A 5×7 bitmap of a label character, one row per byte, bit 4 the leftmost column. Labels are
/// short (a number, a cell, an arrow), so a built-in font keeps marking free of font files.
/// Letters keep their case (a code is copied from the picture by hand: `G1x1` is not `G1X1`);
/// Turkish letters are drawn as their plain forms; anything else is `?`.
fn glyph(c: char) -> [u8; 7] {
    let c = match c {
        'Ç' => 'C',
        'Ğ' => 'G',
        'İ' => 'I',
        'Ö' => 'O',
        'Ş' => 'S',
        'Ü' => 'U',
        'ç' => 'c',
        'ğ' => 'g',
        'ö' => 'o',
        'ş' => 's',
        'ü' => 'u',
        c => c,
    };
    match c {
        'a' => [0x00, 0x00, 0x0e, 0x01, 0x0f, 0x11, 0x0f],
        'b' => [0x10, 0x10, 0x16, 0x19, 0x11, 0x11, 0x1e],
        'c' => [0x00, 0x00, 0x0e, 0x10, 0x10, 0x11, 0x0e],
        'd' => [0x01, 0x01, 0x0d, 0x13, 0x11, 0x11, 0x0f],
        'e' => [0x00, 0x00, 0x0e, 0x11, 0x1f, 0x10, 0x0e],
        'f' => [0x06, 0x09, 0x08, 0x1c, 0x08, 0x08, 0x08],
        'g' => [0x00, 0x00, 0x0f, 0x11, 0x0f, 0x01, 0x0e],
        'h' => [0x10, 0x10, 0x16, 0x19, 0x11, 0x11, 0x11],
        'i' => [0x04, 0x00, 0x0c, 0x04, 0x04, 0x04, 0x0e],
        'ı' => [0x00, 0x00, 0x0c, 0x04, 0x04, 0x04, 0x0e],
        'j' => [0x02, 0x00, 0x06, 0x02, 0x02, 0x12, 0x0c],
        'k' => [0x10, 0x10, 0x12, 0x14, 0x18, 0x14, 0x12],
        'l' => [0x0c, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0e],
        'm' => [0x00, 0x00, 0x1a, 0x15, 0x15, 0x11, 0x11],
        'n' => [0x00, 0x00, 0x16, 0x19, 0x11, 0x11, 0x11],
        'o' => [0x00, 0x00, 0x0e, 0x11, 0x11, 0x11, 0x0e],
        'p' => [0x00, 0x00, 0x1e, 0x11, 0x1e, 0x10, 0x10],
        'q' => [0x00, 0x00, 0x0d, 0x13, 0x0f, 0x01, 0x01],
        'r' => [0x00, 0x00, 0x16, 0x19, 0x10, 0x10, 0x10],
        's' => [0x00, 0x00, 0x0e, 0x10, 0x0e, 0x01, 0x1e],
        't' => [0x08, 0x08, 0x1c, 0x08, 0x08, 0x09, 0x06],
        'u' => [0x00, 0x00, 0x11, 0x11, 0x11, 0x13, 0x0d],
        'v' => [0x00, 0x00, 0x11, 0x11, 0x11, 0x0a, 0x04],
        'w' => [0x00, 0x00, 0x11, 0x11, 0x15, 0x15, 0x0a],
        'x' => [0x00, 0x00, 0x11, 0x0a, 0x04, 0x0a, 0x11],
        'y' => [0x00, 0x00, 0x11, 0x11, 0x0f, 0x01, 0x0e],
        'z' => [0x00, 0x00, 0x1f, 0x02, 0x04, 0x08, 0x1f],
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

fn line(img: &mut image::RgbImage, a: (f64, f64), b: (f64, f64), t: f64, color: image::Rgb<u8>) {
    let steps = (b.0 - a.0).abs().max((b.1 - a.1).abs()).ceil().max(1.0);
    for i in 0..=steps as u32 {
        let k = f64::from(i) / steps;
        dot(img, a.0 + (b.0 - a.0) * k, a.1 + (b.1 - a.1) * k, t, color);
    }
}

/// A label's size in pixels at scale `s`: 6 columns per character (one of them space) and 8 rows
/// per line (one of them space), and a red margin of `2s` around.
fn label_size(lines: &[String], s: f64) -> (f64, f64) {
    let n = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0) as f64;
    let rows = lines.len().max(1) as f64;
    (
        (n * 6.0 - 1.0) * s + 4.0 * s,
        (rows * 8.0 - 1.0) * s + 4.0 * s,
    )
}

/// `text` broken into lines no wider than `max_w` at scale `s`: at spaces, and inside a word only
/// when the word alone is too wide. The flag says a word had to be broken.
fn wrap(text: &str, s: f64, max_w: f64) -> (Vec<String>, bool) {
    // (6n + 3)s ≤ max_w: the characters that fit on one line, at least one.
    let per = (((max_w / s) - 3.0) / 6.0).floor().max(1.0) as usize;
    let (mut lines, mut broken) = (Vec::<String>::new(), false);
    let mut cur = String::new();
    for word in text.split_whitespace() {
        let mut chars: Vec<char> = word.chars().collect();
        while chars.len() > per {
            broken = true;
            if !cur.is_empty() {
                lines.push(std::mem::take(&mut cur));
            }
            lines.push(chars.drain(..per).collect());
        }
        let word: String = chars.into_iter().collect();
        let len = cur.chars().count();
        if len > 0 && len + 1 + word.chars().count() > per {
            lines.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(&word);
    }
    if !cur.is_empty() || lines.is_empty() {
        lines.push(cur);
    }
    (lines, broken)
}

/// The lines and scale a label is drawn at so it is no wider than `max_w`: the photo's own
/// scale `base` when it fits on one line, else a smaller scale (down to a third of `base`) or up
/// to three lines broken at spaces, else the smallest of those scales with its words broken.
/// A bare number keeps `base`; a long label stays as wide as the frame it names.
fn fit_label(text: &str, base: f64, max_w: f64) -> (Vec<String>, f64) {
    let min = (base / 3.0).round().max(1.0);
    let mut s = base;
    while s >= min {
        let (lines, broken) = wrap(text, s, max_w);
        if !broken && lines.len() <= 3 {
            return (lines, s);
        }
        s -= 1.0;
    }
    (wrap(text, min, max_w).0, min)
}

/// White text on a red plate edged in dark, its top-left corner at `(x, y)`, kept inside the
/// image.
fn label(img: &mut image::RgbImage, lines: &[String], x: f64, y: f64, s: f64) {
    let (w, h) = label_size(lines, s);
    let (iw, ih) = (img.width() as f64, img.height() as f64);
    let (x, y) = (
        x.clamp(0.0, (iw - w).max(0.0)),
        y.clamp(0.0, (ih - h).max(0.0)),
    );
    let edge = (s / 2.0).round().max(1.0);
    for py in (y - edge).max(0.0) as u32..((y + h + edge).min(ih)) as u32 {
        for px in (x - edge).max(0.0) as u32..((x + w + edge).min(iw)) as u32 {
            img.put_pixel(px, py, MARK_DARK);
        }
    }
    for py in y as u32..((y + h).min(ih)) as u32 {
        for px in x as u32..((x + w).min(iw)) as u32 {
            img.put_pixel(px, py, MARK_RED);
        }
    }
    for (li, text) in lines.iter().enumerate() {
        let ly = y + 2.0 * s + li as f64 * 8.0 * s;
        for (i, c) in text.chars().enumerate() {
            let g = glyph(c);
            let gx = x + 2.0 * s + i as f64 * 6.0 * s;
            for (row, bits) in g.iter().enumerate() {
                for col in 0..5 {
                    if bits & (0x10 >> col) != 0 {
                        let (cx, cy) = (gx + col as f64 * s, ly + row as f64 * s);
                        dot(img, cx + s / 2.0, cy + s / 2.0, s, MARK_WHITE);
                    }
                }
            }
        }
    }
}

type Area = (f64, f64, f64, f64);

/// Where a label of `size` goes in a photo of `photo` size: the first of `tries` that stays
/// inside the photo, clear of the labels already `placed` and clear of the other `frames`
/// (x, y, w, h); else the first clear of the labels alone. When none is, the first try's column
/// is searched downwards, then upwards, for a row clear of the labels; the first try when there
/// is none. Labels never cover each other where there is room; a frame only gives way to them.
fn label_spot(
    tries: &[(f64, f64)],
    placed: &[Area],
    frames: &[Area],
    size: (f64, f64),
    photo: (f64, f64),
) -> (f64, f64) {
    let (lw, lh) = size;
    let fits = |(x, y): (f64, f64), avoid: &[&[Area]]| {
        let inside = x >= 0.0 && y >= 0.0 && y + lh <= photo.1 && x + lw <= photo.0;
        let clear = avoid
            .iter()
            .flat_map(|a| a.iter())
            .all(|&(px, py, pw, ph)| x + lw <= px || px + pw <= x || y + lh <= py || py + ph <= y);
        inside && clear
    };
    let first = |avoid: &[&[Area]]| tries.iter().copied().find(|&p| fits(p, avoid));
    if let Some(spot) = first(&[placed, frames]).or_else(|| first(&[placed])) {
        return spot;
    }
    let (x, y0) = tries[0];
    let x = x.clamp(0.0, (photo.0 - lw).max(0.0));
    let step = (lh / 4.0).max(1.0);
    let rows = (photo.1 / step).ceil() as usize;
    (1..=rows)
        .flat_map(|k| [y0 + k as f64 * step, y0 - k as f64 * step])
        .map(|y| (x, y))
        .find(|&p| fits(p, &[placed]))
        .unwrap_or(tries[0])
}

/// Draws each `(label, shape)` on a copy of `file` and writes it to `out` as a JPEG: a red frame
/// around the shape, and the label on a red plate — above a rectangle's top-left corner (below
/// it, or inside it, when that is off the photo, on another label or on another frame), in the
/// middle of a grid's cells. Every frame is drawn before any label, so no frame crosses a label.
/// A label is no wider than its frame (or an eighth of the photo, so a number on a small frame
/// stays legible): a long one is drawn smaller or on several lines. The original is not touched.
pub(crate) fn draw_marks(file: &Path, marks: &[(String, Shape)], out: &Path) -> Result<()> {
    let mut img = open_upright(file)?.to_rgb8();
    let (w, h) = (f64::from(img.width()), f64::from(img.height()));
    let short = w.min(h);
    let t = (short / 180.0).max(3.0);
    let base = (short / 150.0).max(2.0).round();
    let corners: Vec<Vec<(f64, f64)>> = marks
        .iter()
        .map(|(_, shape)| {
            let pts: [(f64, f64); 4] = match shape {
                Shape::Rect(c) => [
                    (c.x, c.y),
                    (c.x + c.w, c.y),
                    (c.x + c.w, c.y + c.h),
                    (c.x, c.y + c.h),
                ],
                Shape::Quad(q) => *q,
            };
            pts.iter().map(|(x, y)| (x * w, y * h)).collect()
        })
        .collect();
    let bounds: Vec<Area> = corners
        .iter()
        .map(|px| {
            let left = px.iter().map(|p| p.0).fold(f64::MAX, f64::min);
            let right = px.iter().map(|p| p.0).fold(f64::MIN, f64::max);
            let top = px.iter().map(|p| p.1).fold(f64::MAX, f64::min);
            let bottom = px.iter().map(|p| p.1).fold(f64::MIN, f64::max);
            (
                left - t,
                top - t,
                right - left + 2.0 * t,
                bottom - top + 2.0 * t,
            )
        })
        .collect();
    // Every frame's dark edge first, then the red strokes over them, so no edge cuts into a
    // red corner: a stroke reads on a red thing or background as well as on any other.
    let edge = (t / 3.0).round().max(1.0);
    for (width, color) in [(t + 2.0 * edge, MARK_DARK), (t, MARK_RED)] {
        for px in &corners {
            for i in 0..4 {
                line(&mut img, px[i], px[(i + 1) % 4], width, color);
            }
        }
    }
    let mut placed: Vec<Area> = Vec::new();
    for (i, ((text, shape), px)) in marks.iter().zip(&corners).enumerate() {
        let others: Vec<Area> = (0..bounds.len())
            .filter(|&j| j != i)
            .map(|j| bounds[j])
            .collect();
        let (lines, s) = match shape {
            Shape::Rect(_) => {
                let room = (bounds[i].2 - 2.0 * t).max(short / 8.0).min(w);
                fit_label(text, base, room)
            }
            // A cell is a box the person is looking into: its label is no taller than about a
            // third of the cell and no wider than it, so the box stays visible under it.
            Shape::Quad(_) => {
                let (cw, ch) = (bounds[i].2 - 4.0 * t, bounds[i].3 - 4.0 * t);
                let cap = (ch * 0.3 / 11.0).floor().clamp(1.0, base);
                fit_label(text, cap, cw.max(1.0))
            }
        };
        let (lw, lh) = label_size(&lines, s);
        let (x, y) = match shape {
            Shape::Rect(_) => {
                // Above the frame, else below it, else inside its top, else inside its bottom.
                // A frame inside another frame takes its label inside itself first: outside it,
                // the label would stand in the other frame and read as that one's.
                let (top, bottom) = (px[0].1, px[2].1);
                let x = px[0].0 - t / 2.0;
                let outside = [(x, top - lh - t), (x, bottom + t)];
                let inside = [(px[0].0 + t, top + t), (px[0].0 + t, bottom - lh - t)];
                let (bx, by, bw, bh) = bounds[i];
                let nested = others.iter().any(|&(ox, oy, ow, oh)| {
                    ox <= bx && oy <= by && bx + bw <= ox + ow && by + bh <= oy + oh
                });
                if nested {
                    let tries: Vec<(f64, f64)> = inside.iter().chain(&outside).copied().collect();
                    // Every spot lies in the other frame: only the labels are to keep clear of.
                    label_spot(&tries, &placed, &[], (lw, lh), (w, h))
                } else {
                    let tries: Vec<(f64, f64)> = outside.iter().chain(&inside).copied().collect();
                    label_spot(&tries, &placed, &others, (lw, lh), (w, h))
                }
            }
            Shape::Quad(_) => {
                // In the cell's top-left corner, inside the frame; the middle when that is taken.
                let cx = px.iter().map(|p| p.0).sum::<f64>() / 4.0;
                let cy = px.iter().map(|p| p.1).sum::<f64>() / 4.0;
                let tries = [
                    (px[0].0 + 1.5 * t, px[0].1 + 1.5 * t),
                    (cx - lw / 2.0, cy - lh / 2.0),
                ];
                label_spot(&tries, &placed, &others, (lw, lh), (w, h))
            }
        };
        placed.push((x, y, lw, lh));
        label(&mut img, &lines, x, y, s);
    }
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir).map_err(io(dir))?;
    }
    let mut buf = Cursor::new(Vec::new());
    img.write_with_encoder(JpegEncoder::new_with_quality(&mut buf, 88))
        .map_err(|e| Error::Internal(format!("encoding marked photo: {e}")))?;
    std::fs::write(out, buf.get_ref()).map_err(io(out))
}

/// One small picture of every crop of `file`, side by side and each labelled, written to `out`
/// as a JPEG: the way to check a whole drawer's cut at a glance instead of opening each crop.
/// Tiles keep their proportions inside a fixed box, six to a row, in the order given.
pub(crate) fn contact_sheet(file: &Path, tiles: &[(String, Crop)], out: &Path) -> Result<()> {
    let img = open_upright(file)?.to_rgb8();
    let (w, h) = (f64::from(img.width()), f64::from(img.height()));
    let cuts = tiles
        .iter()
        .map(|(text, c)| {
            let px = |v: f64, max: f64| (v * max).round().clamp(0.0, max) as u32;
            let (x, y) = (px(c.x, w), px(c.y, h));
            let cw = px(c.w, w).min(img.width().saturating_sub(x)).max(1);
            let ch = px(c.h, h).min(img.height().saturating_sub(y)).max(1);
            (
                text.clone(),
                image::imageops::crop_imm(&img, x, y, cw, ch).to_image(),
            )
        })
        .collect::<Vec<_>>();
    write_sheet(&cuts, 6, (240, 200), out)
}

/// Several photos on one sheet, four to a row, each titled: a batch of the series as one
/// image for the agent (spec/series-grid.md).
pub(crate) fn photos_sheet(photos: &[(String, &Path)], out: &Path) -> Result<()> {
    let pictures = photos
        .iter()
        .map(|(text, file)| Ok((text.clone(), open_upright(file)?.to_rgb8())))
        .collect::<Result<Vec<_>>>()?;
    write_sheet(&pictures, 4, (360, 300), out)
}

/// Lays labelled pictures out in rows of `cols`, each fitted in a `tile` box and keeping its
/// proportions, and writes the sheet to `out` as a JPEG.
fn write_sheet(
    pictures: &[(String, image::RgbImage)],
    cols: u32,
    tile: (u32, u32),
    out: &Path,
) -> Result<()> {
    const GAP: u32 = 8;
    const S: f64 = 2.0;
    let strip = label_size(&["X".to_string()], S).1 as u32 + 4;
    let (cell_w, cell_h) = (tile.0 + GAP, tile.1 + strip + GAP);
    let n = pictures.len().max(1) as u32;
    let (shown_cols, rows) = (n.min(cols), n.div_ceil(cols));
    let mut sheet = image::RgbImage::from_pixel(
        shown_cols * cell_w + GAP,
        rows * cell_h + GAP,
        image::Rgb([40, 40, 40]),
    );
    for (i, (text, picture)) in pictures.iter().enumerate() {
        let (col, row) = (i as u32 % cols, i as u32 / cols);
        let (x0, y0) = (GAP + col * cell_w, GAP + row * cell_h);
        let (pw, ph) = (picture.width().max(1), picture.height().max(1));
        let scale = (f64::from(tile.0) / f64::from(pw)).min(f64::from(tile.1) / f64::from(ph));
        let (tw, th) = (
            ((f64::from(pw) * scale).round() as u32).max(1),
            ((f64::from(ph) * scale).round() as u32).max(1),
        );
        let thumb = image::imageops::resize(picture, tw, th, image::imageops::FilterType::Triangle);
        image::imageops::replace(
            &mut sheet,
            &thumb,
            i64::from(x0 + (tile.0 - tw) / 2),
            i64::from(y0 + strip + (tile.1 - th) / 2),
        );
        // The title is cut to its tile, so a long note does not run over the next one.
        let (lines, _) = wrap(text, S, f64::from(tile.0));
        let first = lines.into_iter().next().unwrap_or_default();
        label(&mut sheet, &[first], f64::from(x0), f64::from(y0), S);
    }
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir).map_err(io(dir))?;
    }
    let mut buf = Cursor::new(Vec::new());
    sheet
        .write_with_encoder(JpegEncoder::new_with_quality(&mut buf, 85))
        .map_err(|e| Error::Internal(format!("encoding contact sheet: {e}")))?;
    std::fs::write(out, buf.get_ref()).map_err(io(out))
}

#[cfg(test)]
mod tests {
    use super::{Crop, Shape, draw_marks, fit_label, glyph, label_size, label_spot};

    #[test]
    fn a_padded_crop_grows_on_every_side_and_stays_inside_the_photo() {
        let c = Crop {
            x: 0.2,
            y: 0.05,
            w: 0.4,
            h: 0.5,
        }
        .padded(0.1);
        assert!((c.x - 0.16).abs() < 1e-9 && c.y.abs() < 1e-9, "{c}");
        assert!((c.w - 0.48).abs() < 1e-9 && (c.h - 0.6).abs() < 1e-9, "{c}");
        let edge = Crop {
            x: 0.9,
            y: 0.9,
            w: 0.1,
            h: 0.1,
        }
        .padded(0.5);
        assert!((edge.x + edge.w - 1.0).abs() < 1e-9 && (edge.y + edge.h - 1.0).abs() < 1e-9);
    }

    #[test]
    fn a_long_label_shrinks_or_wraps_to_its_frame_and_a_number_keeps_its_size() {
        let long = "1 LR44 Mettzchrom x17 (#484)";
        for max_w in [120.0, 300.0, 600.0] {
            let (lines, s) = fit_label(long, 27.0, max_w);
            let (w, _) = label_size(&lines, s);
            assert!(w <= max_w, "{w} > {max_w} for {lines:?} at {s}");
            assert!(s >= 1.0);
        }
        // Wide enough: one line at the photo's scale. A bare number never shrinks.
        assert_eq!(fit_label(long, 9.0, 2000.0), (vec![long.to_string()], 9.0));
        assert_eq!(fit_label("12", 27.0, 500.0), (vec!["12".to_string()], 27.0));
        // Broken at spaces before a smaller scale than a third of the photo's.
        let (lines, s) = fit_label(long, 27.0, 600.0);
        assert!(lines.len() > 1 && s >= 9.0, "{lines:?} at {s}");
    }

    #[test]
    fn a_label_with_no_listed_spot_free_moves_down_its_column() {
        let size = (100.0, 40.0);
        let photo = (1000.0, 800.0);
        let placed = [(300.0, 100.0, 100.0, 40.0), (300.0, 400.0, 100.0, 40.0)];
        let tries = [(300.0, 100.0), (300.0, 400.0)];
        let (x, y) = label_spot(&tries, &placed, &[], size, photo);
        assert_eq!(x, 300.0);
        assert!(y >= 140.0 && y + 40.0 <= 800.0, "{y}");
        assert!(
            placed
                .iter()
                .all(|&(_, py, _, ph)| y + 40.0 <= py || py + ph <= y)
        );
    }

    #[test]
    fn a_label_steps_off_another_frame_but_not_onto_another_label() {
        let size = (100.0, 40.0);
        let photo = (1000.0, 800.0);
        let tries = [(300.0, 100.0), (300.0, 400.0)];
        // Another frame where the label would go above: below instead.
        let frames = [(250.0, 50.0, 300.0, 80.0)];
        let spot = label_spot(&tries, &[], &frames, size, photo);
        assert_eq!(spot, (300.0, 400.0));
        // A label below as well: over the frame rather than over the label.
        let placed = [(300.0, 400.0, 100.0, 40.0)];
        let spot = label_spot(&tries, &placed, &frames, size, photo);
        assert_eq!(spot, (300.0, 100.0));
    }

    #[test]
    fn a_long_label_is_drawn_within_its_frames_width_and_inside_the_photo() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("p.png");
        // A 2000×1500 grey photo: the scale of a phone photo, where labels came out as bars.
        image::RgbImage::from_pixel(2000, 1500, image::Rgb([128, 128, 128]))
            .save(&file)
            .unwrap();
        let out = dir.path().join("m.jpg");
        let frame = Crop {
            x: 0.4,
            y: 0.0,
            w: 0.2,
            h: 0.3,
        };
        let marks = [
            (
                "1 LR44 Mettzchrom x17 (#484)".to_string(),
                Shape::Rect(frame),
            ),
            ("2 CR2032 Varta x3 (#485)".to_string(), Shape::Rect(frame)),
        ];
        draw_marks(&file, &marks, &out).unwrap();
        let img = image::open(&out).unwrap().to_rgb8();
        let red = |p: &image::Rgb<u8>| p[0] > 180 && p[1] < 90 && p[2] < 90;
        let (mut lo, mut hi) = (u32::MAX, 0);
        for (x, _, p) in img.enumerate_pixels() {
            if red(p) {
                lo = lo.min(x);
                hi = hi.max(x);
            }
        }
        // The frame spans x 800–1200 and its line is about 8 px thick; nothing red outside it.
        assert!(lo >= 790 && hi <= 1210, "red from x {lo} to {hi}");
    }

    #[test]
    fn a_cell_label_sits_in_its_corner_and_leaves_the_box_to_be_seen() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("p.png");
        image::RgbImage::from_pixel(2000, 1500, image::Rgb([128, 128, 128]))
            .save(&file)
            .unwrap();
        let out = dir.path().join("m.jpg");
        // One cell of a drawer: x 400–800, y 300–600 on a phone-sized photo.
        let cell = Shape::Quad([(0.2, 0.2), (0.4, 0.2), (0.4, 0.4), (0.2, 0.4)]);
        draw_marks(&file, &[("001".to_string(), cell)], &out).unwrap();
        let img = image::open(&out).unwrap().to_rgb8();
        let red = |x: u32, y: u32| {
            let p = img.get_pixel(x, y);
            p[0] > 180 && p[1] < 90 && p[2] < 90
        };
        // The label's plate is in the top-left corner; the cell's middle and lower part are clear.
        assert!(red(430, 330), "the label is in the corner");
        assert!(!red(600, 450), "the box under the label shows");
        assert!(!red(600, 540));
    }

    #[test]
    fn a_frame_inside_another_has_its_label_inside_itself() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("p.png");
        image::RgbImage::from_pixel(2000, 1500, image::Rgb([128, 128, 128]))
            .save(&file)
            .unwrap();
        let out = dir.path().join("m.jpg");
        let rect = |x, y, w, h| Shape::Rect(Crop { x, y, w, h });
        // A cable inside the headphones' frame, in its top-left corner: above it is label 1.
        let marks = [
            ("1".to_string(), rect(0.2, 0.2, 0.6, 0.6)),
            ("2".to_string(), rect(0.21, 0.21, 0.2, 0.2)),
        ];
        draw_marks(&file, &marks, &out).unwrap();
        let img = image::open(&out).unwrap().to_rgb8();
        let red = |x: u32, y: u32| {
            let p = img.get_pixel(x, y);
            p[0] > 180 && p[1] < 90 && p[2] < 90
        };
        // The inner frame spans x 420–820, y 315–615: its label is in its top-left corner, not
        // below it in the outer frame, where it would read as the outer one's.
        assert!(red(450, 345), "the label is inside its frame");
        assert!(!red(450, 660), "nothing below the inner frame");
    }

    #[test]
    fn a_label_steps_aside_from_one_already_drawn_and_stays_in_the_photo() {
        let size = (100.0, 40.0);
        let photo = (1000.0, 800.0);
        let tries = [(300.0, 100.0), (300.0, 400.0), (310.0, 150.0)];
        // Nothing drawn yet: above the frame.
        assert_eq!(label_spot(&tries, &[], &[], size, photo), (300.0, 100.0));
        // A label already there: below the frame instead.
        let placed = [(350.0, 90.0, 100.0, 40.0)];
        assert_eq!(
            label_spot(&tries, &placed, &[], size, photo),
            (300.0, 400.0)
        );
        // Above the photo's top edge does not count as a place.
        let tries = [(300.0, -20.0), (300.0, 400.0)];
        assert_eq!(label_spot(&tries, &[], &[], size, photo), (300.0, 400.0));
    }

    #[test]
    fn label_letters_keep_their_case_fold_turkish_and_mark_the_unknown() {
        assert_eq!(glyph('ü'), glyph('u'));
        assert_eq!(glyph('Ş'), glyph('S'));
        // A code is copied from the picture by hand: `G1x1` must not read `G1X1`.
        assert_ne!(glyph('x'), glyph('X'));
        assert_eq!(glyph('€'), glyph('?'));
        assert_ne!(glyph('1'), glyph('?'));
    }

    #[test]
    fn crops_parse_and_must_fit() {
        assert!("0.1,0.2,0.3,0.4".parse::<Crop>().is_ok());
        assert!("0.5,0,0.6,1".parse::<Crop>().is_err());
        assert!("0,0,0,1".parse::<Crop>().is_err());
        assert!("a,b,c,d".parse::<Crop>().is_err());
        assert!("0,0,1".parse::<Crop>().is_err());
    }
}
