//! `cargo xtask gen-media`: builds a small synthetic media library with real naming
//! conventions plus `manifest.json` describing what each file should probe and parse as.

use std::process::Command;

use anyhow::{Context, bail};
use camino::{Utf8Path, Utf8PathBuf};
use serde_json::{Value, json};

#[derive(Debug, clap::Args)]
pub struct Args {
    /// Short (3 s) clips and skip the slow 4K sample.
    #[arg(long)]
    quick: bool,
    /// Regenerate files that already exist.
    #[arg(long)]
    force: bool,
    /// Output directory (default: test-media/out).
    #[arg(long)]
    out: Option<Utf8PathBuf>,
}

struct Ctx {
    dur: u32,
    tmp: Utf8PathBuf,
}

struct Expect {
    kind: &'static str,
    container: &'static str,
    video: Option<&'static str>,
    audio: &'static [&'static str],
    subtitles: &'static [&'static str],
    size: Option<(u32, u32)>,
    hdr: bool,
    interlaced: bool,
}

struct Spec {
    path: &'static str,
    full_only: bool,
    sidecar_srt: Option<&'static str>,
    args: fn(&Ctx) -> Vec<String>,
    expect: Expect,
    parse: fn() -> Value,
}

const SRT: &str = "1\n00:00:00,500 --> 00:00:02,000\nHello from MBL\n\n2\n00:00:02,000 --> 00:00:03,000\nSecond line\n";
const ASS: &str = "[Script Info]\nScriptType: v4.00+\nPlayResX: 1280\nPlayResY: 720\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: Default,Arial,48,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,2,0,2,10,10,10,1\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:00.50,0:00:02.00,Default,,0,0,0,,{\\b1}Styled{\\b0} subtitle\n";

const HDR10_X265: &str = "log-level=error:hdr10=1:repeat-headers=1:colorprim=bt2020:transfer=smpte2084:colormatrix=bt2020nc:master-display=G(13250,34500)B(7500,3000)R(34000,16000)WP(15635,16450)L(10000000,1):max-cll=1000,400";

fn strs(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| (*s).to_owned()).collect()
}

fn video(ctx: &Ctx, w: u32, h: u32, rate: u32) -> Vec<String> {
    vec![
        "-f".into(),
        "lavfi".into(),
        "-i".into(),
        format!("testsrc2=size={w}x{h}:rate={rate}:duration={}", ctx.dur),
    ]
}

fn sine(ctx: &Ctx, freq: u32) -> Vec<String> {
    vec![
        "-f".into(),
        "lavfi".into(),
        "-i".into(),
        format!(
            "sine=frequency={freq}:sample_rate=48000:duration={}",
            ctx.dur
        ),
    ]
}

fn file_input(path: &Utf8Path) -> Vec<String> {
    vec!["-i".into(), path.to_string()]
}

fn h264_aac(ctx: &Ctx) -> Vec<String> {
    [
        video(ctx, 1280, 720, 24),
        sine(ctx, 440),
        strs(&[
            "-map",
            "0:v",
            "-map",
            "1:a",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
        ]),
        strs(&[
            "-pix_fmt", "yuv420p", "-c:a", "aac", "-b:a", "128k", "-ac", "2",
        ]),
    ]
    .concat()
}

fn music_tags(artist: &str, album: &str, title: &str, track: &str) -> Vec<String> {
    vec![
        "-metadata".into(),
        format!("artist={artist}"),
        "-metadata".into(),
        format!("album_artist={artist}"),
        "-metadata".into(),
        format!("album={album}"),
        "-metadata".into(),
        format!("title={title}"),
        "-metadata".into(),
        format!("track={track}"),
        "-metadata".into(),
        "date=2020".into(),
    ]
}

fn specs() -> Vec<Spec> {
    vec![
        Spec {
            path: "Movies/Big Test (2020)/Big Test (2020).mp4",
            full_only: false,
            sidecar_srt: Some("Movies/Big Test (2020)/Big Test (2020).en.srt"),
            args: |ctx| [h264_aac(ctx), strs(&["-movflags", "+faststart"])].concat(),
            expect: Expect {
                kind: "movie",
                container: "mov,mp4,m4a,3gp,3g2,mj2",
                video: Some("h264"),
                audio: &["aac"],
                subtitles: &[],
                size: Some((1280, 720)),
                hdr: false,
                interlaced: false,
            },
            parse: || json!({ "title": "Big Test", "year": 2020 }),
        },
        Spec {
            path: "Movies/Subtitle Test (2018)/Subtitle Test (2018).mkv",
            full_only: false,
            sidecar_srt: None,
            args: |ctx| {
                [
                    video(ctx, 1280, 720, 24),
                    sine(ctx, 440),
                    file_input(&ctx.tmp.join("sub.srt")),
                    file_input(&ctx.tmp.join("sub.ass")),
                    strs(&["-map", "0:v", "-map", "1:a", "-map", "2", "-map", "3"]),
                    strs(&[
                        "-c:v",
                        "libx264",
                        "-preset",
                        "ultrafast",
                        "-pix_fmt",
                        "yuv420p",
                    ]),
                    strs(&["-c:a", "ac3", "-ac", "6", "-c:s:0", "srt", "-c:s:1", "ass"]),
                    strs(&[
                        "-metadata:s:s:0",
                        "language=eng",
                        "-metadata:s:s:1",
                        "language=eng",
                    ]),
                    strs(&["-metadata:s:a:0", "language=eng"]),
                ]
                .concat()
            },
            expect: Expect {
                kind: "movie",
                container: "matroska,webm",
                video: Some("h264"),
                audio: &["ac3"],
                subtitles: &["subrip", "ass"],
                size: Some((1280, 720)),
                hdr: false,
                interlaced: false,
            },
            parse: || json!({ "title": "Subtitle Test", "year": 2018 }),
        },
        Spec {
            path: "Movies/HDR Test (2021) {tmdb-12345}/HDR Test (2021) {tmdb-12345}.mkv",
            full_only: false,
            sidecar_srt: None,
            args: |ctx| {
                [
                    video(ctx, 1280, 720, 24),
                    sine(ctx, 440),
                    strs(&[
                        "-map",
                        "0:v",
                        "-map",
                        "1:a",
                        "-c:v",
                        "libx265",
                        "-preset",
                        "ultrafast",
                    ]),
                    strs(&["-pix_fmt", "yuv420p10le", "-x265-params", HDR10_X265]),
                    strs(&["-color_primaries", "bt2020", "-color_trc", "smpte2084"]),
                    strs(&["-colorspace", "bt2020nc", "-c:a", "aac", "-ac", "2"]),
                ]
                .concat()
            },
            expect: Expect {
                kind: "movie",
                container: "matroska,webm",
                video: Some("hevc"),
                audio: &["aac"],
                subtitles: &[],
                size: Some((1280, 720)),
                hdr: true,
                interlaced: false,
            },
            parse: || json!({ "title": "HDR Test", "year": 2021, "tmdb": 12345 }),
        },
        Spec {
            path: "Movies/DTS Test (2017)/DTS Test (2017).mkv",
            full_only: false,
            sidecar_srt: None,
            args: |ctx| {
                [
                    video(ctx, 1280, 720, 24),
                    sine(ctx, 440),
                    strs(&[
                        "-map",
                        "0:v",
                        "-map",
                        "1:a",
                        "-c:v",
                        "libx265",
                        "-preset",
                        "ultrafast",
                    ]),
                    strs(&["-x265-params", "log-level=error", "-pix_fmt", "yuv420p"]),
                    strs(&["-c:a", "dca", "-strict", "-2", "-ac", "6"]),
                ]
                .concat()
            },
            expect: Expect {
                kind: "movie",
                container: "matroska,webm",
                video: Some("hevc"),
                audio: &["dts"],
                subtitles: &[],
                size: Some((1280, 720)),
                hdr: false,
                interlaced: false,
            },
            parse: || json!({ "title": "DTS Test", "year": 2017 }),
        },
        Spec {
            path: "Movies/AV1 Test (2023)/AV1 Test (2023).mkv",
            full_only: false,
            sidecar_srt: None,
            args: |ctx| {
                [
                    video(ctx, 1280, 720, 24),
                    sine(ctx, 440),
                    strs(&[
                        "-map",
                        "0:v",
                        "-map",
                        "1:a",
                        "-c:v",
                        "libsvtav1",
                        "-preset",
                        "12",
                    ]),
                    strs(&["-pix_fmt", "yuv420p", "-c:a", "libopus", "-b:a", "96k"]),
                ]
                .concat()
            },
            expect: Expect {
                kind: "movie",
                container: "matroska,webm",
                video: Some("av1"),
                audio: &["opus"],
                subtitles: &[],
                size: Some((1280, 720)),
                hdr: false,
                interlaced: false,
            },
            parse: || json!({ "title": "AV1 Test", "year": 2023 }),
        },
        Spec {
            path: "Movies/VP9 Test (2016)/VP9 Test (2016).webm",
            full_only: false,
            sidecar_srt: None,
            args: |ctx| {
                [
                    video(ctx, 1280, 720, 24),
                    sine(ctx, 440),
                    strs(&[
                        "-map",
                        "0:v",
                        "-map",
                        "1:a",
                        "-c:v",
                        "libvpx-vp9",
                        "-deadline",
                    ]),
                    strs(&[
                        "realtime",
                        "-cpu-used",
                        "8",
                        "-b:v",
                        "1M",
                        "-c:a",
                        "libopus",
                    ]),
                ]
                .concat()
            },
            expect: Expect {
                kind: "movie",
                container: "matroska,webm",
                video: Some("vp9"),
                audio: &["opus"],
                subtitles: &[],
                size: Some((1280, 720)),
                hdr: false,
                interlaced: false,
            },
            parse: || json!({ "title": "VP9 Test", "year": 2016 }),
        },
        Spec {
            path: "Movies/Multi Audio (2019)/Multi Audio (2019).mkv",
            full_only: false,
            sidecar_srt: None,
            args: |ctx| {
                [
                    video(ctx, 1280, 720, 24),
                    sine(ctx, 440),
                    sine(ctx, 660),
                    strs(&["-map", "0:v", "-map", "1:a", "-map", "2:a"]),
                    strs(&[
                        "-c:v",
                        "libx264",
                        "-preset",
                        "ultrafast",
                        "-pix_fmt",
                        "yuv420p",
                    ]),
                    strs(&["-c:a", "aac", "-ac", "2"]),
                    strs(&[
                        "-metadata:s:a:0",
                        "language=eng",
                        "-metadata:s:a:1",
                        "language=ger",
                    ]),
                    strs(&["-disposition:a:0", "default", "-disposition:a:1", "0"]),
                ]
                .concat()
            },
            expect: Expect {
                kind: "movie",
                container: "matroska,webm",
                video: Some("h264"),
                audio: &["aac", "aac"],
                subtitles: &[],
                size: Some((1280, 720)),
                hdr: false,
                interlaced: false,
            },
            parse: || json!({ "title": "Multi Audio", "year": 2019 }),
        },
        Spec {
            path: "Movies/UHD Test (2022)/UHD Test (2022) - 2160p.mkv",
            full_only: true,
            sidecar_srt: None,
            args: |ctx| {
                [
                    video(ctx, 3840, 2160, 24),
                    sine(ctx, 440),
                    strs(&[
                        "-map",
                        "0:v",
                        "-map",
                        "1:a",
                        "-c:v",
                        "libx264",
                        "-preset",
                        "ultrafast",
                    ]),
                    strs(&["-pix_fmt", "yuv420p", "-c:a", "aac", "-ac", "2"]),
                ]
                .concat()
            },
            expect: Expect {
                kind: "movie",
                container: "matroska,webm",
                video: Some("h264"),
                audio: &["aac"],
                subtitles: &[],
                size: Some((3840, 2160)),
                hdr: false,
                interlaced: false,
            },
            parse: || json!({ "title": "UHD Test", "year": 2022 }),
        },
        Spec {
            path: "TV/Test Show (2019)/Season 01/Test Show - S01E01 - Pilot.mkv",
            full_only: false,
            sidecar_srt: None,
            args: h264_aac,
            expect: episode_expect(),
            parse: || json!({ "show": "Test Show", "year": 2019, "season": 1, "episodes": [1], "title": "Pilot" }),
        },
        Spec {
            path: "TV/Test Show (2019)/Season 01/Test Show - S01E02 - Second.mkv",
            full_only: false,
            sidecar_srt: None,
            args: h264_aac,
            expect: episode_expect(),
            parse: || json!({ "show": "Test Show", "year": 2019, "season": 1, "episodes": [2], "title": "Second" }),
        },
        Spec {
            path: "TV/Test Show (2019)/Season 01/Test Show - S01E03-E04 - Double.mkv",
            full_only: false,
            sidecar_srt: None,
            args: h264_aac,
            expect: episode_expect(),
            parse: || json!({ "show": "Test Show", "year": 2019, "season": 1, "episodes": [3, 4], "title": "Double" }),
        },
        Spec {
            path: "TV/Interlaced Show/Season 01/Interlaced Show - S01E01.ts",
            full_only: false,
            sidecar_srt: None,
            args: |ctx| {
                [
                    video(ctx, 720, 576, 25),
                    sine(ctx, 440),
                    strs(&["-map", "0:v", "-map", "1:a", "-vf", "setfield=tff"]),
                    strs(&["-c:v", "mpeg2video", "-flags", "+ilme+ildct", "-top", "1"]),
                    strs(&["-b:v", "4M", "-c:a", "ac3", "-ac", "2", "-f", "mpegts"]),
                ]
                .concat()
            },
            expect: Expect {
                kind: "episode",
                container: "mpegts",
                video: Some("mpeg2video"),
                audio: &["ac3"],
                subtitles: &[],
                size: Some((720, 576)),
                hdr: false,
                interlaced: true,
            },
            parse: || json!({ "show": "Interlaced Show", "season": 1, "episodes": [1] }),
        },
        Spec {
            path: "Music/Test Artist/Test Album (2020)/01 - First Song.flac",
            full_only: false,
            sidecar_srt: None,
            args: |ctx| {
                [
                    sine(ctx, 440),
                    strs(&["-c:a", "flac"]),
                    music_tags("Test Artist", "Test Album", "First Song", "1/3"),
                ]
                .concat()
            },
            expect: track_expect("flac", "flac"),
            parse: || json!({ "artist": "Test Artist", "album": "Test Album", "track": 1, "title": "First Song" }),
        },
        Spec {
            path: "Music/Test Artist/Test Album (2020)/02 - Second Song.mp3",
            full_only: false,
            sidecar_srt: None,
            args: |ctx| {
                [
                    sine(ctx, 550),
                    strs(&["-c:a", "libmp3lame", "-b:a", "192k"]),
                    music_tags("Test Artist", "Test Album", "Second Song", "2/3"),
                ]
                .concat()
            },
            expect: track_expect("mp3", "mp3"),
            parse: || json!({ "artist": "Test Artist", "album": "Test Album", "track": 2, "title": "Second Song" }),
        },
        Spec {
            path: "Music/Test Artist/Test Album (2020)/03 - Third Song.m4a",
            full_only: false,
            sidecar_srt: None,
            args: |ctx| {
                [
                    sine(ctx, 660),
                    strs(&["-c:a", "aac", "-b:a", "192k"]),
                    music_tags("Test Artist", "Test Album", "Third Song", "3/3"),
                ]
                .concat()
            },
            expect: track_expect("mov,mp4,m4a,3gp,3g2,mj2", "aac"),
            parse: || json!({ "artist": "Test Artist", "album": "Test Album", "track": 3, "title": "Third Song" }),
        },
    ]
}

fn episode_expect() -> Expect {
    Expect {
        kind: "episode",
        container: "matroska,webm",
        video: Some("h264"),
        audio: &["aac"],
        subtitles: &[],
        size: Some((1280, 720)),
        hdr: false,
        interlaced: false,
    }
}

fn track_expect(container: &'static str, codec: &'static str) -> Expect {
    let audio: &'static [&'static str] = match codec {
        "flac" => &["flac"],
        "mp3" => &["mp3"],
        _ => &["aac"],
    };
    Expect {
        kind: "track",
        container,
        video: None,
        audio,
        subtitles: &[],
        size: None,
        hdr: false,
        interlaced: false,
    }
}

pub fn run(args: &Args) -> anyhow::Result<()> {
    ensure_tool("ffmpeg")?;
    ensure_tool("ffprobe")?;

    let out = args
        .out
        .clone()
        .unwrap_or_else(|| crate::workspace_root().join("test-media/out"));
    let ctx = Ctx {
        dur: if args.quick { 3 } else { 10 },
        tmp: out.join(".tmp"),
    };
    std::fs::create_dir_all(&ctx.tmp)?;
    std::fs::write(ctx.tmp.join("sub.srt"), SRT)?;
    std::fs::write(ctx.tmp.join("sub.ass"), ASS)?;

    let mut manifest = Vec::new();
    for spec in specs() {
        if args.quick && spec.full_only {
            eprintln!("skip  {} (--quick)", spec.path);
            continue;
        }
        let target = out.join(spec.path);
        if args.force || !target.exists() {
            eprintln!("make  {}", spec.path);
            generate(&ctx, &spec, &target)?;
        } else {
            eprintln!("keep  {}", spec.path);
        }
        if let Some(srt) = spec.sidecar_srt {
            std::fs::write(out.join(srt), SRT)?;
        }
        verify(&target, &spec.expect).with_context(|| format!("verifying {}", spec.path))?;
        manifest.push(manifest_entry(&spec));
    }

    std::fs::remove_dir_all(&ctx.tmp)?;
    let doc = json!({ "version": 1, "durationSecs": ctx.dur, "files": manifest });
    std::fs::write(
        out.join("manifest.json"),
        serde_json::to_string_pretty(&doc)?,
    )?;
    eprintln!(
        "done: {} files, manifest at {}",
        doc["files"].as_array().map_or(0, Vec::len),
        out.join("manifest.json")
    );
    Ok(())
}

fn ensure_tool(name: &str) -> anyhow::Result<()> {
    let ok = Command::new(name)
        .arg("-version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !ok {
        bail!("`{name}` not found on PATH; install FFmpeg >= 6.1");
    }
    Ok(())
}

fn generate(ctx: &Ctx, spec: &Spec, target: &Utf8Path) -> anyhow::Result<()> {
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let output = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-y"])
        .args((spec.args)(ctx))
        .args(["-t", &ctx.dur.to_string()])
        .arg(target.as_str())
        .output()
        .context("running ffmpeg")?;
    if !output.status.success() {
        let _ = std::fs::remove_file(target);
        bail!(
            "ffmpeg failed for {}:\n{}",
            spec.path,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

fn verify(path: &Utf8Path, expect: &Expect) -> anyhow::Result<()> {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_format",
            "-show_streams",
            "-of",
            "json",
        ])
        .arg(path.as_str())
        .output()
        .context("running ffprobe")?;
    if !output.status.success() {
        bail!(
            "ffprobe failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let probe: Value = serde_json::from_slice(&output.stdout)?;
    let streams = probe["streams"].as_array().cloned().unwrap_or_default();
    let codecs = |kind: &str| -> Vec<String> {
        streams
            .iter()
            .filter(|s| s["codec_type"] == kind)
            .filter(|s| s["disposition"]["attached_pic"] != 1)
            .filter_map(|s| s["codec_name"].as_str().map(str::to_owned))
            .collect()
    };

    let container = probe["format"]["format_name"].as_str().unwrap_or_default();
    if container != expect.container {
        bail!("container {container:?}, expected {:?}", expect.container);
    }
    let video = codecs("video");
    let expected_video: Vec<String> = expect.video.iter().map(|s| (*s).to_owned()).collect();
    if video != expected_video {
        bail!("video codecs {video:?}, expected {expected_video:?}");
    }
    if codecs("audio") != expect.audio {
        bail!(
            "audio codecs {:?}, expected {:?}",
            codecs("audio"),
            expect.audio
        );
    }
    if codecs("subtitle") != expect.subtitles {
        bail!(
            "subtitle codecs {:?}, expected {:?}",
            codecs("subtitle"),
            expect.subtitles
        );
    }
    if let (Some((w, h)), Some(v)) = (
        expect.size,
        streams.iter().find(|s| s["codec_type"] == "video"),
    ) {
        if v["width"] != w || v["height"] != h {
            bail!("size {}x{}, expected {w}x{h}", v["width"], v["height"]);
        }
        let hdr = v["color_transfer"] == "smpte2084" || v["color_transfer"] == "arib-std-b67";
        if hdr != expect.hdr {
            bail!("hdr={hdr}, expected {}", expect.hdr);
        }
        let interlaced = matches!(v["field_order"].as_str(), Some("tt" | "bb" | "tb" | "bt"));
        if interlaced != expect.interlaced {
            bail!("interlaced={interlaced}, expected {}", expect.interlaced);
        }
    }
    Ok(())
}

fn manifest_entry(spec: &Spec) -> Value {
    let e = &spec.expect;
    json!({
        "path": spec.path,
        "kind": e.kind,
        "sidecarSubtitles": spec.sidecar_srt.into_iter().collect::<Vec<_>>(),
        "expected": {
            "container": e.container,
            "videoCodec": e.video,
            "audioCodecs": e.audio,
            "subtitleCodecs": e.subtitles,
            "width": e.size.map(|s| s.0),
            "height": e.size.map(|s| s.1),
            "hdr": e.hdr,
            "interlaced": e.interlaced,
        },
        "parse": (spec.parse)(),
    })
}
