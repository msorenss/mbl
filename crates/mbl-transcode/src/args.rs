//! Pure FFmpeg argument builder (`build_args`) from a typed [`Pipeline`].

use std::ffi::OsString;

use mbl_core::{Error, Result};

use crate::escape::{escape_filter_path, validate_filter_graph};
use crate::pipeline::{AudioOp, Backend, FfmpegCaps, Pipeline, SubtitleOp, VideoCodec, VideoOp};

/// Build the exact `ffmpeg` argv vector for `p` without invoking a shell.
pub fn build_args(p: &Pipeline, caps: &FfmpegCaps) -> Result<Vec<OsString>> {
    validate_pipeline(p, caps)?;

    let mut args: Vec<OsString> = Vec::with_capacity(48);
    push_str(&mut args, "-nostdin");
    push_str(&mut args, "-hide_banner");
    push_pair(&mut args, "-loglevel", "error");
    push_str(&mut args, "-y");

    if let Some(threads) = caps.threads {
        push_pair(&mut args, "-threads", &threads.to_string());
    }

    if p.start_ms > 0 {
        push_pair(&mut args, "-ss", &format_timestamp_ms(p.start_ms));
        push_str(&mut args, "-copyts");
        push_pair(&mut args, "-avoid_negative_ts", "disabled");
    }

    push_pair(&mut args, "-i", &format_input_uri(p.input.as_str()));

    // Video mapping, filters, and codec options.
    match &p.video {
        VideoOp::None => {
            push_str(&mut args, "-vn");
        }
        VideoOp::Copy => {
            push_pair(&mut args, "-map", "0:v:0");
            push_pair(&mut args, "-c:v", "copy");
        }
        VideoOp::Encode { codec, width, max_bitrate, deinterlace, tonemap: _ } => {
            match &p.subtitle {
                SubtitleOp::BurnImage { index } => {
                    let graph = build_burn_image_graph(*deinterlace, *width, *index)?;
                    push_pair(&mut args, "-filter_complex", &graph);
                    push_pair(&mut args, "-map", "[vout]");
                }
                SubtitleOp::BurnText { index } => {
                    let vf = build_video_filter_chain(
                        *deinterlace,
                        *width,
                        Some((p.input.as_str(), *index)),
                    )?;
                    push_pair(&mut args, "-map", "0:v:0");
                    push_pair(&mut args, "-vf", &vf);
                }
                SubtitleOp::None => {
                    let vf = build_video_filter_chain(*deinterlace, *width, None)?;
                    push_pair(&mut args, "-map", "0:v:0");
                    push_pair(&mut args, "-vf", &vf);
                }
            }

            push_software_video_encoder(
                &mut args,
                *codec,
                *max_bitrate,
                p.output.segment_secs,
                caps,
            );
        }
    }

    // Audio mapping and codec options.
    match &p.audio {
        AudioOp::Copy { index } => {
            push_pair(&mut args, "-map", &format!("0:{index}"));
            push_pair(&mut args, "-c:a", "copy");
        }
        AudioOp::Encode { index, codec, channels, bitrate } => {
            push_pair(&mut args, "-map", &format!("0:{index}"));
            push_pair(&mut args, "-c:a", codec.ffmpeg_encoder());
            push_pair(&mut args, "-ac", &channels.to_string());
            push_pair(&mut args, "-b:a", &bitrate.to_string());
        }
    }

    // Never mux raw subtitle tracks into fMP4 segments; subtitles are either sidecar or burned in.
    push_str(&mut args, "-sn");

    // HLS fMP4 output flags.
    let target_dir = ffmpeg_path(p.output.rendition_dir(0).as_str());
    let segment_pattern = format!("{target_dir}/%d.m4s");
    let playlist_path = format!("{target_dir}/ffmpeg.m3u8");

    push_pair(&mut args, "-f", "hls");
    push_pair(&mut args, "-hls_time", &p.output.segment_secs.to_string());
    push_pair(&mut args, "-hls_playlist_type", "event");
    push_pair(&mut args, "-hls_segment_type", "fmp4");
    push_pair(&mut args, "-hls_fmp4_init_filename", "init.mp4");
    push_pair(&mut args, "-start_number", &p.output.start_number.to_string());
    push_pair(&mut args, "-hls_segment_filename", &segment_pattern);
    push_str(&mut args, &playlist_path);

    Ok(args)
}

fn ffmpeg_path(path: &str) -> String {
    #[cfg(windows)]
    {
        path.replace('\\', "/")
    }
    #[cfg(not(windows))]
    {
        path.to_owned()
    }
}

fn validate_pipeline(p: &Pipeline, caps: &FfmpegCaps) -> Result<()> {
    if p.input.as_str().is_empty() || p.input.as_str().contains('\0') {
        return Err(Error::Validation("pipeline input path is invalid".into()));
    }
    if p.output.dir.as_str().is_empty() || p.output.dir.as_str().contains('\0') {
        return Err(Error::Validation("pipeline output dir is invalid".into()));
    }
    if p.output.segment_secs == 0 {
        return Err(Error::Validation("HLS segment_secs must be >= 1".into()));
    }
    if p.backend != Backend::Software && matches!(p.video, VideoOp::Encode { .. }) {
        return Err(Error::Validation(format!(
            "hardware backend {:?} is not supported in software pipeline builder",
            p.backend
        )));
    }
    if !matches!(p.subtitle, SubtitleOp::None) && !matches!(p.video, VideoOp::Encode { .. }) {
        return Err(Error::Validation(
            "subtitle burn-in requires video encoding (VideoOp::Encode)".into(),
        ));
    }
    if let VideoOp::Encode { width, max_bitrate, .. } = &p.video {
        if *max_bitrate == 0 {
            return Err(Error::Validation("video max_bitrate must be > 0".into()));
        }
        if matches!(width, Some(0)) {
            return Err(Error::Validation("video width must be > 0 when specified".into()));
        }
    }
    if let AudioOp::Encode { channels, bitrate, .. } = &p.audio {
        if *channels == 0 {
            return Err(Error::Validation("audio channels must be >= 1".into()));
        }
        if *bitrate == 0 {
            return Err(Error::Validation("audio bitrate must be > 0".into()));
        }
    }
    if caps.preset.trim().is_empty() {
        return Err(Error::Validation("ffmpeg preset must not be empty".into()));
    }
    Ok(())
}

fn build_video_filter_chain(
    deinterlace: bool,
    width: Option<u32>,
    burn_text: Option<(&str, u32)>,
) -> Result<String> {
    let mut filters: Vec<String> = Vec::with_capacity(4);
    if deinterlace {
        filters.push("yadif=mode=send_frame:parity=auto:deint=all".to_owned());
    }
    if let Some(w) = width {
        filters.push(format!("scale=w={w}:h=-2"));
    }
    if let Some((input_path, sub_index)) = burn_text {
        let escaped = escape_filter_path(input_path)?;
        filters.push(format!("subtitles=filename={escaped}:si={sub_index}"));
    }
    filters.push("format=yuv420p".to_owned());

    let chain = filters.join(",");
    validate_filter_graph(&chain)?;
    Ok(chain)
}

fn build_burn_image_graph(
    deinterlace: bool,
    width: Option<u32>,
    subtitle_stream_index: u32,
) -> Result<String> {
    let mut tail = String::from("overlay=eof_action=pass");
    if let Some(w) = width {
        tail.push_str(&format!(",scale=w={w}:h=-2"));
    }
    tail.push_str(",format=yuv420p[vout]");

    let graph = if deinterlace {
        format!(
            "[0:v:0]yadif=mode=send_frame:parity=auto:deint=all[v0];[v0][0:{subtitle_stream_index}]{tail}"
        )
    } else {
        format!("[0:v:0][0:{subtitle_stream_index}]{tail}")
    };
    validate_filter_graph(&graph)?;
    Ok(graph)
}

fn push_software_video_encoder(
    args: &mut Vec<OsString>,
    codec: VideoCodec,
    max_bitrate: u64,
    segment_secs: u32,
    caps: &FfmpegCaps,
) {
    let bufsize = max_bitrate.saturating_mul(2);
    match codec {
        VideoCodec::H264 => {
            push_pair(args, "-c:v", "libx264");
            push_pair(args, "-preset", &caps.preset);
            push_pair(args, "-crf", &caps.crf.to_string());
            push_pair(args, "-maxrate", &max_bitrate.to_string());
            push_pair(args, "-bufsize", &bufsize.to_string());
            push_pair(args, "-pix_fmt", "yuv420p");
            push_pair(args, "-sc_threshold", "0");
        }
        VideoCodec::Hevc => {
            push_pair(args, "-c:v", "libx265");
            push_pair(args, "-preset", &caps.preset);
            push_pair(args, "-crf", &caps.crf.to_string());
            push_pair(args, "-maxrate", &max_bitrate.to_string());
            push_pair(args, "-bufsize", &bufsize.to_string());
            push_pair(args, "-pix_fmt", "yuv420p");
            push_pair(args, "-tag:v", "hvc1");
        }
        VideoCodec::Av1 => {
            push_pair(args, "-c:v", "libsvtav1");
            push_pair(args, "-crf", &caps.crf.to_string());
            push_pair(args, "-maxrate", &max_bitrate.to_string());
            push_pair(args, "-bufsize", &bufsize.to_string());
            push_pair(args, "-pix_fmt", "yuv420p");
        }
    }
    push_pair(args, "-force_key_frames", &format!("expr:gte(t,n_forced*{segment_secs})"));
}

fn format_timestamp_ms(ms: u64) -> String {
    let secs = ms / 1000;
    let rem_ms = ms % 1000;
    format!("{secs}.{rem_ms:03}")
}

fn format_input_uri(input: &str) -> String {
    if input.starts_with("http://") || input.starts_with("https://") || input.starts_with("file:") {
        input.to_owned()
    } else {
        format!("file:{input}")
    }
}

fn push_str(args: &mut Vec<OsString>, val: &str) {
    args.push(OsString::from(val));
}

fn push_pair(args: &mut Vec<OsString>, flag: &str, val: &str) {
    args.push(OsString::from(flag));
    args.push(OsString::from(val));
}
