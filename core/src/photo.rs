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
