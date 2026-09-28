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

