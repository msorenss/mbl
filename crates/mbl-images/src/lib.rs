//! Artwork fetching, resizing, BlurHash and palette extraction.
//!
//! Originals are stored once under `<cache>/images/orig/<sha256>`; resized JPEG variants live in
//! `<cache>/images/<width>/<sha256>-q<quality>.jpg` with widths snapped to a fixed set.

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::time::Duration;

use image::{DynamicImage, GenericImageView, ImageReader, imageops::FilterType};
use mbl_core::{Error, Result};
use serde::Serialize;
use sha2::{Digest, Sha256};

/// Allowed output widths; requests are snapped to the nearest one at or above.
pub const WIDTHS: &[u32] = &[160, 240, 320, 480, 720, 1080, 1920];
const MAX_DOWNLOAD: u64 = 20 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Palette {
    pub dominant: String,
    pub vibrant: String,
}

/// Result of ingesting an original.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ingested {
    pub hash: String,
    pub width: u32,
    pub height: u32,
    pub blurhash: String,
    pub palette: Palette,
}

#[derive(Debug, Clone)]
pub struct ImageStore {
    root: PathBuf,
    http: reqwest::Client,
}

pub fn snap_width(w: u32) -> u32 {
    WIDTHS.iter().copied().find(|s| *s >= w).unwrap_or(WIDTHS[WIDTHS.len() - 1])
}

impl ImageStore {
    pub fn new(cache_dir: &Path) -> Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent(concat!("MBL/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(Error::internal)?;
        Ok(Self { root: cache_dir.join("images"), http })
    }

    fn original_path(&self, hash: &str) -> PathBuf {
        self.root.join("orig").join(hash)
    }

    /// Download (`http(s):`) or read (`file:`) an image, validate it and store the original.
    pub async fn ingest(&self, source_url: &str) -> Result<Ingested> {
        let bytes = if let Some(path) = source_url.strip_prefix("file:") {
            let path = PathBuf::from(path);
            tokio::task::spawn_blocking(move || read_capped(&path))
                .await
                .map_err(Error::internal)??
        } else if source_url.starts_with("https://") || source_url.starts_with("http://") {
            self.download(source_url).await?
        } else {
            return Err(Error::Validation(format!("unsupported artwork URL: {source_url}")));
        };
        let root = self.root.clone();
        tokio::task::spawn_blocking(move || {
            let (img, _) = decode(&bytes)?;
            let hash: String = Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect();
            let path = root.join("orig").join(&hash);
            if !path.exists() {
                write_atomic(&path, &bytes)?;
            }
            let (width, height) = img.dimensions();
            let small = img.thumbnail(64, 64).to_rgba8();
            let blurhash = blurhash::encode(4, 3, small.width(), small.height(), small.as_raw())
                .map_err(|e| Error::Validation(format!("blurhash failed: {e:?}")))?;
            Ok(Ingested { hash, width, height, blurhash, palette: palette(&small) })
        })
        .await
        .map_err(Error::internal)?
    }

    async fn download(&self, url: &str) -> Result<Vec<u8>> {
        let mut res = self
            .http
            .get(url)
            .send()
            .await
            .map_err(|e| Error::External(format!("image download failed: {e}")))?;
        if !res.status().is_success() {
            return Err(Error::External(format!("image download returned {}", res.status())));
        }
        if res.content_length().is_some_and(|l| l > MAX_DOWNLOAD) {
            return Err(Error::Validation("image too large".into()));
        }
        let mut out = Vec::new();
        while let Some(chunk) = res.chunk().await.map_err(|e| Error::External(e.to_string()))? {
            out.extend_from_slice(&chunk);
            if out.len() as u64 > MAX_DOWNLOAD {
                return Err(Error::Validation("image too large".into()));
            }
        }
        Ok(out)
    }

    /// Path to a JPEG of the original resized to `width` (snapped), creating it if needed.
    pub async fn variant(&self, hash: &str, width: u32, quality: u8) -> Result<PathBuf> {
        if hash.len() != 64 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(Error::Validation("invalid image hash".into()));
        }
        let width = snap_width(width);
        let quality = quality.clamp(30, 95);
        let target = self.root.join(width.to_string()).join(format!("{hash}-q{quality}.jpg"));
        if target.exists() {
            return Ok(target);
        }
        let original = self.original_path(hash);
        tokio::task::spawn_blocking(move || {
            let bytes = std::fs::read(&original).map_err(|_| Error::NotFound)?;
            let (img, _) = decode(&bytes)?;
            let resized = if img.width() > width {
                let height = (u64::from(img.height()) * u64::from(width)
                    / u64::from(img.width().max(1))) as u32;
                img.resize_exact(width, height.max(1), FilterType::Triangle)
            } else {
                img
            };
            let mut out = Vec::new();
            let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality);
            resized.to_rgb8().write_with_encoder(encoder).map_err(Error::internal)?;
            write_atomic(&target, &out)?;
            Ok(target)
        })
        .await
        .map_err(Error::internal)?
    }

    /// Path to a pre-blurred JPEG of the original (`?blur=1`), cached on disk so TV/low-power
    /// clients never need expensive CSS `backdrop-filter: blur()`.
    pub async fn blurred_variant(&self, hash: &str, width: u32, quality: u8) -> Result<PathBuf> {
        if hash.len() != 64 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(Error::Validation("invalid image hash".into()));
        }
        let width = snap_width(width);
        let quality = quality.clamp(30, 95);
        let target = self.root.join(width.to_string()).join(format!("{hash}-q{quality}-blur.jpg"));
        if target.exists() {
            return Ok(target);
        }
        let original = self.original_path(hash);
        tokio::task::spawn_blocking(move || {
            let bytes = std::fs::read(&original).map_err(|_| Error::NotFound)?;
            let (img, _) = decode(&bytes)?;
            let out_w = width.min(img.width().max(1));
            let out_h = (u64::from(img.height()) * u64::from(out_w) / u64::from(img.width().max(1)))
                .max(1) as u32;
            // Downscale before Gaussian blur for O(1) latency on Raspberry Pi, then scale to output.
            let blur_w = out_w.min(240);
            let blur_h =
                (u64::from(out_h) * u64::from(blur_w) / u64::from(out_w.max(1))).max(1) as u32;
            let blurred = img.resize_exact(blur_w, blur_h, FilterType::Triangle).blur(14.0);
            let final_img = if out_w > blur_w {
                blurred.resize_exact(out_w, out_h, FilterType::Triangle)
            } else {
                blurred
            };
            let mut out = Vec::new();
            let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality);
            final_img.to_rgb8().write_with_encoder(encoder).map_err(Error::internal)?;
            write_atomic(&target, &out)?;
            Ok(target)
        })
        .await
        .map_err(Error::internal)?
    }
}

fn read_capped(path: &Path) -> Result<Vec<u8>> {
    let meta = std::fs::metadata(path).map_err(|_| Error::NotFound)?;
    if !meta.is_file() || meta.len() > MAX_DOWNLOAD {
        return Err(Error::Validation("not a usable image file".into()));
    }
    Ok(std::fs::read(path)?)
}

fn decode(bytes: &[u8]) -> Result<(DynamicImage, image::ImageFormat)> {
    let reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| Error::Validation(e.to_string()))?;
    let format = reader.format().ok_or_else(|| Error::Validation("not an image".into()))?;
    let img = reader.decode().map_err(|e| Error::Validation(format!("not a valid image: {e}")))?;
    Ok((img, format))
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension(format!("tmp{}", std::process::id()));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Dominant = most common color bucket; vibrant = the bucket with the best saturation × count.
pub fn palette(img: &image::RgbaImage) -> Palette {
    let mut buckets: std::collections::HashMap<(u8, u8, u8), (u32, [u64; 3])> = Default::default();
    for p in img.pixels() {
        if p[3] < 128 {
            continue;
        }
        let key = (p[0] >> 4, p[1] >> 4, p[2] >> 4);
        let entry = buckets.entry(key).or_insert((0, [0; 3]));
        entry.0 += 1;
        for c in 0..3 {
            entry.1[c] += u64::from(p[c]);
        }
    }
    let mean = |(n, sum): &(u32, [u64; 3])| -> [u8; 3] {
        let n = u64::from((*n).max(1));
        [(sum[0] / n) as u8, (sum[1] / n) as u8, (sum[2] / n) as u8]
    };
    let saturation = |c: [u8; 3]| -> f32 {
        let max = f32::from(*c.iter().max().unwrap_or(&0));
        let min = f32::from(*c.iter().min().unwrap_or(&0));
        if max == 0.0 { 0.0 } else { (max - min) / max }
    };
    let hex = |c: [u8; 3]| format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2]);
    let dominant = buckets.values().max_by_key(|b| b.0).map(mean).unwrap_or([0, 0, 0]);
    let vibrant = buckets
        .values()
        .map(|b| (mean(b), b.0))
        .max_by(|a, b| {
            (saturation(a.0) * (a.1 as f32).sqrt())
                .total_cmp(&(saturation(b.0) * (b.1 as f32).sqrt()))
        })
        .map_or(dominant, |(c, _)| c);
    Palette { dominant: hex(dominant), vibrant: hex(vibrant) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(w: u32, h: u32) -> Vec<u8> {
        let img = image::RgbaImage::from_fn(w, h, |x, _| {
            if x < w / 2 { image::Rgba([20, 20, 20, 255]) } else { image::Rgba([220, 30, 30, 255]) }
        });
        let mut out = Vec::new();
        DynamicImage::ImageRgba8(img)
            .write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png)
            .unwrap();
        out
    }

    #[test]
    fn snapping() {
        assert_eq!(snap_width(1), 160);
        assert_eq!(snap_width(300), 320);
        assert_eq!(snap_width(480), 480);
        assert_eq!(snap_width(5000), 1920);
    }

    #[tokio::test]
    async fn ingest_resize_and_cache() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("poster.png");
        std::fs::write(&src, png(600, 900)).unwrap();
        let store = ImageStore::new(dir.path()).unwrap();

        let ing = store.ingest(&format!("file:{}", src.display())).await.unwrap();
        assert_eq!((ing.width, ing.height), (600, 900));
        assert_eq!(ing.hash.len(), 64);
        assert!(!ing.blurhash.is_empty());
        assert_eq!(ing.palette.vibrant, "#dc1e1e");

        let v = store.variant(&ing.hash, 300, 80).await.unwrap();
        assert!(v.to_str().unwrap().contains("/320/"));
        let (img, format) = decode(&std::fs::read(&v).unwrap()).unwrap();
        assert_eq!(format, image::ImageFormat::Jpeg);
        assert_eq!(img.dimensions(), (320, 480));
        let mtime = std::fs::metadata(&v).unwrap().modified().unwrap();
        let again = store.variant(&ing.hash, 320, 80).await.unwrap();
        assert_eq!(
            std::fs::metadata(&again).unwrap().modified().unwrap(),
            mtime,
            "served from cache"
        );

        let small = store.variant(&ing.hash, 1920, 80).await.unwrap();
        assert_eq!(
            decode(&std::fs::read(small).unwrap()).unwrap().0.width(),
            600,
            "never upscales"
        );
    }

    #[tokio::test]
    async fn rejects_non_images_and_bad_hashes() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("fake.jpg");
        std::fs::write(&src, b"definitely not an image").unwrap();
        let store = ImageStore::new(dir.path()).unwrap();
        assert!(matches!(
            store.ingest(&format!("file:{}", src.display())).await,
            Err(Error::Validation(_))
        ));
        assert!(matches!(store.ingest("ftp://x/y.jpg").await, Err(Error::Validation(_))));
        assert!(matches!(
            store.variant("../../etc/passwd", 320, 80).await,
            Err(Error::Validation(_))
        ));
        assert!(matches!(store.variant(&"0".repeat(64), 320, 80).await, Err(Error::NotFound)));
    }

    #[tokio::test]
    async fn downloads_over_http() {
        use wiremock::matchers::path;
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let server = MockServer::start().await;
        Mock::given(path("/p.png"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(png(10, 10)))
            .mount(&server)
            .await;
        Mock::given(path("/missing.png"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().unwrap();
        let store = ImageStore::new(dir.path()).unwrap();
        assert_eq!(store.ingest(&format!("{}/p.png", server.uri())).await.unwrap().width, 10);
        assert!(matches!(
            store.ingest(&format!("{}/missing.png", server.uri())).await,
            Err(Error::External(_))
        ));
    }
}
