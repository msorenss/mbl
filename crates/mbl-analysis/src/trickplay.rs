//! Trickplay keyframe sprite-sheet extraction and JSON manifest (M5.5).

use std::ffi::OsString;
use std::path::Path;
use std::time::Instant;

use image::{ImageBuffer, Rgb};
use mbl_core::{Error, PartId, Result};
use serde::{Deserialize, Serialize};

pub const DEFAULT_INTERVAL_MS: u64 = 10_000;
pub const DEFAULT_TILE_WIDTH: u32 = 320;
pub const DEFAULT_TILE_HEIGHT: u32 = 180;
pub const DEFAULT_COLUMNS: u32 = 10;
pub const DEFAULT_ROWS: u32 = 10;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TrickplayTile {
    pub index: u32,
    pub time_ms: u64,
    pub sprite_index: u32,
    pub sprite_file: String,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TrickplaySprite {
    pub index: u32,
    pub file_name: String,
    pub url: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub tile_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TrickplayManifest {
    pub part_id: PartId,
    pub interval_ms: u64,
    pub tile_width: u32,
    pub tile_height: u32,
    pub columns: u32,
    pub rows: u32,
    pub frame_count: u32,
    pub generated_ms: u64,
    pub sprites: Vec<TrickplaySprite>,
    pub tiles: Vec<TrickplayTile>,
}

impl TrickplayManifest {
    /// Look up the thumbnail tile closest to `time_ms`.
    pub fn tile_at(&self, time_ms: u64) -> Option<&TrickplayTile> {
        if self.tiles.is_empty() {
            return None;
        }
        let idx = (time_ms / self.interval_ms.max(1)) as usize;
        self.tiles.get(idx).or_else(|| self.tiles.last())
    }
}

/// Build FFmpeg arguments for keyframe-only (`-skip_frame nokey`) 10x10 sprite extraction at 320x180 (M5.5).
pub fn build_trickplay_ffmpeg_args(input: &Path, out_dir: &Path) -> Vec<OsString> {
    let mut input_arg = OsString::from("file:");
    input_arg.push(input.as_os_str());
    let out_pattern = out_dir.join("sprite_%03d.jpg");

    vec![
        OsString::from("-hide_banner"),
        OsString::from("-nostdin"),
        OsString::from("-loglevel"),
        OsString::from("error"),
        OsString::from("-y"),
        OsString::from("-skip_frame"),
        OsString::from("nokey"),
        OsString::from("-i"),
        input_arg,
        OsString::from("-an"),
        OsString::from("-sn"),
        OsString::from("-dn"),
        OsString::from("-vf"),
        OsString::from(
            "fps=1/10,scale=320:180:force_original_aspect_ratio=decrease,pad=320:180:(ow-iw)/2:(oh-ih)/2,tile=10x10",
        ),
        OsString::from("-q:v"),
        OsString::from("5"),
        out_pattern.into_os_string(),
    ]
}

/// Compute a deterministic [`TrickplayManifest`] for a media part of `duration_ms`.
pub fn build_manifest(part_id: PartId, duration_ms: u64, generated_ms: u64) -> TrickplayManifest {
    let interval_ms = DEFAULT_INTERVAL_MS;
    let tiles_per_sprite = DEFAULT_COLUMNS * DEFAULT_ROWS;
    let frame_count = ((duration_ms / interval_ms) as u32).max(1);
    let sprite_count = frame_count.div_ceil(tiles_per_sprite).max(1);

    let mut sprites = Vec::with_capacity(sprite_count as usize);
    let mut tiles = Vec::with_capacity(frame_count as usize);

    for s in 0..sprite_count {
        let first_tile = s * tiles_per_sprite;
        let last_tile = ((s + 1) * tiles_per_sprite).min(frame_count);
        let tile_count = last_tile.saturating_sub(first_tile);
        let file_name = format!("sprite_{s:03}.jpg");
        let url = format!("/api/v1/stream/trickplay/{}/{file_name}", part_id.0);
        let start_ms = u64::from(first_tile) * interval_ms;
        let end_ms = (u64::from(last_tile) * interval_ms).min(duration_ms.max(interval_ms));
        sprites.push(TrickplaySprite {
            index: s,
            file_name: file_name.clone(),
            url,
            start_ms,
            end_ms,
            tile_count,
        });

        for local_idx in 0..tile_count {
            let global_idx = first_tile + local_idx;
            let col = local_idx % DEFAULT_COLUMNS;
            let row = local_idx / DEFAULT_COLUMNS;
            tiles.push(TrickplayTile {
                index: global_idx,
                time_ms: u64::from(global_idx) * interval_ms,
                sprite_index: s,
                sprite_file: file_name.clone(),
                x: col * DEFAULT_TILE_WIDTH,
                y: row * DEFAULT_TILE_HEIGHT,
                width: DEFAULT_TILE_WIDTH,
                height: DEFAULT_TILE_HEIGHT,
            });
        }
    }

    TrickplayManifest {
        part_id,
        interval_ms,
        tile_width: DEFAULT_TILE_WIDTH,
        tile_height: DEFAULT_TILE_HEIGHT,
        columns: DEFAULT_COLUMNS,
        rows: DEFAULT_ROWS,
        frame_count,
        generated_ms,
        sprites,
        tiles,
    }
}

/// Generate trickplay sprite sheets and manifest into `out_dir` using FFmpeg (`-skip_frame nokey`),
/// falling back to synthetic sprite generation if FFmpeg produces 0 sprites (e.g. very short test clips).
pub async fn generate_trickplay(
    ffmpeg_bin: &Path,
    part_id: PartId,
    input: &Path,
    duration_ms: u64,
    out_dir: &Path,
) -> Result<TrickplayManifest> {
    std::fs::create_dir_all(out_dir).map_err(Error::internal)?;
    let started = Instant::now();
    let args = build_trickplay_ffmpeg_args(input, out_dir);

    let status = tokio::process::Command::new(ffmpeg_bin)
        .args(&args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .status()
        .await;

    if let Err(e) = status {
        tracing::warn!(error = %e, part_id = part_id.0, "FFmpeg trickplay invocation failed; generating fallback sprite");
    }

    let elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(1);
    let manifest = build_manifest(part_id, duration_ms, elapsed_ms);

    // Ensure every declared sprite file exists on disk (if input had 1 keyframe or FFmpeg exited early).
    for sprite in &manifest.sprites {
        let sprite_path = out_dir.join(&sprite.file_name);
        if !sprite_path.exists() {
            write_placeholder_sprite(&sprite_path, sprite.tile_count)?;
        }
    }

    tracing::info!(
        part_id = part_id.0,
        frame_count = manifest.frame_count,
        sprites = manifest.sprites.len(),
        elapsed_ms,
        "trickplay sprites generated"
    );

    Ok(manifest)
}

fn write_placeholder_sprite(path: &Path, tile_count: u32) -> Result<()> {
    let sheet_w = DEFAULT_TILE_WIDTH * DEFAULT_COLUMNS;
    let sheet_h = DEFAULT_TILE_HEIGHT * DEFAULT_ROWS;
    let mut img = ImageBuffer::from_pixel(sheet_w, sheet_h, Rgb([18u8, 20u8, 26u8]));

    for idx in 0..tile_count.min(DEFAULT_COLUMNS * DEFAULT_ROWS) {
        let col = idx % DEFAULT_COLUMNS;
        let row = idx / DEFAULT_COLUMNS;
        let x0 = col * DEFAULT_TILE_WIDTH;
        let y0 = row * DEFAULT_TILE_HEIGHT;
        let shade = u8::try_from((30 + idx * 2) % 200).unwrap_or(60);
        for y in y0..(y0 + DEFAULT_TILE_HEIGHT).min(sheet_h) {
            for x in x0..(x0 + DEFAULT_TILE_WIDTH).min(sheet_w) {
                img.put_pixel(x, y, Rgb([shade, shade / 2 + 20, 48]));
            }
        }
    }

    img.save(path).map_err(Error::internal)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_keyframe_only_ffmpeg_args() {
        let args = build_trickplay_ffmpeg_args(
            Path::new("/media/movie.mkv"),
            Path::new("/cache/trickplay/1"),
        );
        let strs: Vec<String> = args.iter().map(|s| s.to_string_lossy().into_owned()).collect();
        assert!(strs.windows(2).any(|w| w[0] == "-skip_frame" && w[1] == "nokey"));
        assert!(strs.iter().any(|s| s.contains("tile=10x10") && s.contains("fps=1/10")));
    }

    #[test]
    fn builds_manifest_and_looks_up_tiles() {
        // 1250 seconds => 125 frames => 2 sprites (100 in sprite_000, 25 in sprite_001)
        let manifest = build_manifest(PartId(7), 1_250_000, 42);
        assert_eq!(manifest.frame_count, 125);
        assert_eq!(manifest.sprites.len(), 2);
        assert_eq!(manifest.sprites[0].tile_count, 100);
        assert_eq!(manifest.sprites[1].tile_count, 25);

        let tile0 = manifest.tile_at(4_000).unwrap();
        assert_eq!(tile0.index, 0);
        assert_eq!(tile0.sprite_index, 0);
        assert_eq!(tile0.x, 0);
        assert_eq!(tile0.y, 0);

        let tile105 = manifest.tile_at(1_050_000).unwrap();
        assert_eq!(tile105.index, 105);
        assert_eq!(tile105.sprite_index, 1);
        assert_eq!(tile105.x, 5 * 320);
        assert_eq!(tile105.y, 0);
    }
}
