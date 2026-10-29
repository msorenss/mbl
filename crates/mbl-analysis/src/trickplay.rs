//! Trickplay keyframe sprite-sheet extraction and JSON manifest (M5.5).

use std::ffi::OsString;
use std::path::Path;
use std::time::Instant;

use image::{ImageBuffer, Rgb};
use mbl_core::{Error, PartId, Result};
use serde::{Deserialize, Serialize};
use std::time::Duration;

pub const DEFAULT_INTERVAL_MS: u64 = 10_000;
pub const DEFAULT_TILE_WIDTH: u32 = 320;
pub const DEFAULT_TILE_HEIGHT: u32 = 180;
pub const DEFAULT_COLUMNS: u32 = 10;
pub const DEFAULT_ROWS: u32 = 10;
/// Bound manifest allocation and output for videos up to 48 hours.
pub const MAX_FRAME_COUNT: u64 = 17_280;

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

/// Sample actual keyframes at 320x180; JPEGs are assembled into sheets after extraction.
pub fn build_trickplay_ffmpeg_args(input: &Path, out_dir: &Path) -> Vec<OsString> {
    let mut input_arg = OsString::from("file:");
    input_arg.push(input.as_os_str());
    let out_pattern = out_dir.join("frame_%06d.jpg");

    vec![
        OsString::from("-hide_banner"),
        OsString::from("-nostdin"),
        OsString::from("-loglevel"),
        OsString::from("error"),
        OsString::from("-y"),
        OsString::from("-threads"),
        OsString::from("1"),
        OsString::from("-skip_frame"),
        OsString::from("nokey"),
        OsString::from("-i"),
        input_arg,
        OsString::from("-an"),
        OsString::from("-sn"),
        OsString::from("-dn"),
        OsString::from("-vf"),
        OsString::from(
            "tpad=stop_mode=clone:stop_duration=10,fps=1/10:start_time=0:round=up,scale=320:180:force_original_aspect_ratio=decrease,pad=320:180:(ow-iw)/2:(oh-ih)/2",
        ),
        OsString::from("-q:v"),
        OsString::from("5"),
        OsString::from("-start_number"),
        OsString::from("0"),
        out_pattern.into_os_string(),
    ]
}

/// Compute a deterministic [`TrickplayManifest`] for a media part of `duration_ms`.
pub fn build_manifest(part_id: PartId, duration_ms: u64, generated_ms: u64) -> TrickplayManifest {
    let interval_ms = DEFAULT_INTERVAL_MS;
    let tiles_per_sprite = DEFAULT_COLUMNS * DEFAULT_ROWS;
    let frame_count = duration_ms.div_ceil(interval_ms).clamp(1, MAX_FRAME_COUNT) as u32;
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

/// Generate real keyframe previews. A failed command or incomplete output is an error;
/// no placeholder files or successful manifest are produced for undecodable sources.
pub async fn generate_trickplay(
    ffmpeg_bin: &Path,
    part_id: PartId,
    input: &Path,
    duration_ms: u64,
    out_dir: &Path,
) -> Result<TrickplayManifest> {
    if duration_ms == 0 || duration_ms > MAX_FRAME_COUNT * DEFAULT_INTERVAL_MS {
        return Err(Error::Validation("trickplay requires a duration within 0..48 hours".into()));
    }
    let frames = out_dir.join("frames");
    tokio::fs::create_dir_all(&frames).await.map_err(Error::internal)?;
    let started = Instant::now();
    let mut args = build_trickplay_ffmpeg_args(input, &frames);
    // Put the frame limit before the image2 output URL.
    let output = args.pop().ok_or_else(|| Error::Validation("missing trickplay output".into()))?;
    args.extend([
        OsString::from("-frames:v"),
        OsString::from(duration_ms.div_ceil(DEFAULT_INTERVAL_MS).to_string()),
        output,
    ]);
    let mut command = tokio::process::Command::new(ffmpeg_bin);
    command
        .args(args)
        .kill_on_drop(true)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let status = tokio::time::timeout(Duration::from_secs(600), command.status())
        .await
        .map_err(|_| Error::Unavailable("trickplay generation timed out".into()))?
        .map_err(|_| Error::Unavailable("could not start FFmpeg for trickplay".into()))?;
    if !status.success() {
        return Err(Error::Unavailable("FFmpeg could not decode trickplay source".into()));
    }
    let elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(1);
    let manifest = build_manifest(part_id, duration_ms, elapsed_ms);
    let result = manifest.clone();
    let out_dir = out_dir.to_path_buf();
    tokio::task::spawn_blocking(move || -> Result<()> {
        for sprite in &manifest.sprites {
            let mut image = ImageBuffer::from_pixel(
                DEFAULT_TILE_WIDTH * DEFAULT_COLUMNS,
                DEFAULT_TILE_HEIGHT * DEFAULT_ROWS,
                Rgb([0u8, 0u8, 0u8]),
            );
            let first = sprite.index * DEFAULT_COLUMNS * DEFAULT_ROWS;
            for local in 0..sprite.tile_count {
                let path = frames.join(format!("frame_{:06}.jpg", first + local));
                let tile = image::open(path)
                    .map_err(|_| Error::Unavailable("incomplete trickplay frames".into()))?
                    .to_rgb8();
                if tile.dimensions() != (DEFAULT_TILE_WIDTH, DEFAULT_TILE_HEIGHT) {
                    return Err(Error::Unavailable("invalid trickplay frame dimensions".into()));
                }
                image::imageops::replace(
                    &mut image,
                    &tile,
                    i64::from(local % DEFAULT_COLUMNS * DEFAULT_TILE_WIDTH),
                    i64::from(local / DEFAULT_COLUMNS * DEFAULT_TILE_HEIGHT),
                );
            }
            image.save(out_dir.join(&sprite.file_name)).map_err(Error::internal)?;
        }
        std::fs::remove_dir_all(frames).map_err(Error::internal)?;
        Ok(())
    })
    .await
    .map_err(Error::internal)??;
    Ok(result)
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
        assert!(strs.iter().any(|s| s.contains("fps=1/10:start_time=0:round=up")));
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

#[cfg(test)]
mod real_media_tests {
    use super::*;

    #[test]
    fn rounds_up_partial_intervals_and_bounds_untrusted_duration() {
        assert_eq!(build_manifest(PartId(1), 100, 1).frame_count, 1);
        assert_eq!(build_manifest(PartId(1), 10_001, 1).frame_count, 2);
        assert_eq!(build_manifest(PartId(1), u64::MAX, 1).frame_count, MAX_FRAME_COUNT as u32);
    }

    #[tokio::test]
    async fn actual_frames_cover_short_clips_and_sprite_boundaries() {
        let tools = mbl_probe::FfTools::locate("", "").await.unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("source.mkv");
        let status = tokio::process::Command::new(&tools.ffmpeg)
            .args([
                "-nostdin",
                "-v",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "color=red:s=64x36:r=1:d=1000",
                "-f",
                "lavfi",
                "-i",
                "color=blue:s=64x36:r=1:d=1",
                "-filter_complex",
                "[0:v][1:v]concat=n=2:v=1:a=0[v]",
                "-map",
                "[v]",
                "-c:v",
                "mpeg4",
                "-g",
                "1",
            ])
            .arg(&source)
            .status()
            .await
            .unwrap();
        assert!(status.success());
        let out = tmp.path().join("sheets");
        let manifest =
            generate_trickplay(&tools.ffmpeg, PartId(1), &source, 1_001_000, &out).await.unwrap();
        assert_eq!(manifest.frame_count, 101);
        assert_eq!(manifest.sprites.len(), 2);
        assert_eq!(manifest.sprites[1].tile_count, 1);
        for sprite in &manifest.sprites {
            let image = image::open(out.join(&sprite.file_name)).unwrap().to_rgb8();
            assert_eq!(image.dimensions(), (3200, 1800));
            let pixel = image.get_pixel(160, 90);
            if sprite.index == 0 {
                assert!(pixel[0] > 180 && pixel[2] < 50, "{pixel:?}");
            } else {
                assert!(pixel[2] > 180 && pixel[0] < 50, "{pixel:?}");
            }
        }
        let short = tmp.path().join("short.mkv");
        assert!(
            tokio::process::Command::new(&tools.ffmpeg)
                .args([
                    "-v",
                    "error",
                    "-y",
                    "-f",
                    "lavfi",
                    "-i",
                    "color=green:s=64x36:r=10:d=0.1",
                    "-c:v",
                    "mpeg4"
                ])
                .arg(&short)
                .status()
                .await
                .unwrap()
                .success()
        );
        let manifest =
            generate_trickplay(&tools.ffmpeg, PartId(2), &short, 100, &tmp.path().join("short"))
                .await
                .unwrap();
        assert_eq!(manifest.frame_count, 1);
    }

    #[tokio::test]
    async fn broken_source_never_produces_placeholder_success() {
        let tools = mbl_probe::FfTools::locate("", "").await.unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let input = tmp.path().join("broken.mkv");
        std::fs::write(&input, b"invalid video").unwrap();
        let out = tmp.path().join("out");
        assert!(generate_trickplay(&tools.ffmpeg, PartId(1), &input, 10_000, &out).await.is_err());
        assert!(!out.join("sprite_000.jpg").exists());
    }
}
