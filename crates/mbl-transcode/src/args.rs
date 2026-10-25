//! Pure FFmpeg argument builder (`build_args`) from a typed [`Pipeline`] across software and
//! hardware backends (`Vaapi`, `Qsv`, `Nvenc`, `V4l2m2m`, `PiDrmDecode`, `D3d11Amf`), HDR
//! tone-mapping filters (M4.10), and GPU multi-rendition ABR ladders (M4.13).

use std::ffi::OsString;

use mbl_core::{Error, Result};

use crate::escape::{escape_filter_path, validate_filter_graph};
use crate::pipeline::{AudioOp, Backend, FfmpegCaps, Pipeline, SubtitleOp, VideoCodec, VideoOp};

/// Build the exact `ffmpeg` argv vector for `p` without invoking a shell.
pub fn build_args(p: &Pipeline, caps: &FfmpegCaps) -> Result<Vec<OsString>> {
    validate_pipeline(p, caps)?;

    let mut args: Vec<OsString> = Vec::with_capacity(64);
    push_str(&mut args, "-nostdin");
    push_str(&mut args, "-hide_banner");
    push_pair(&mut args, "-loglevel", "error");
    push_str(&mut args, "-y");

    if let Some(threads) = caps.threads {
        push_pair(&mut args, "-threads", &threads.to_string());
    }

    // Hardware input decode flags before `-i` (M4.4 – M4.8).
    if matches!(p.video, VideoOp::Encode { .. }) {
        push_input_hwaccel(&mut args, p.backend);
    }

    if p.start_ms > 0 {
        push_pair(&mut args, "-ss", &format_timestamp_ms(p.start_ms));
        push_str(&mut args, "-copyts");
        push_pair(&mut args, "-avoid_negative_ts", "disabled");
    }

    push_pair(&mut args, "-i", &format_input_uri(p.input.as_str()));

    // Multi-rendition GPU ABR ladder (M4.13).
    if p.output.renditions.len() > 1
        && let VideoOp::Encode { codec, deinterlace, tonemap, .. } = &p.video
    {
        build_abr_ladder_args(&mut args, p, caps, *codec, *deinterlace, *tonemap)?;
        return Ok(args);
    }

    // Single-rendition video mapping, filters, and codec options.
    match &p.video {
        VideoOp::None => {
            push_str(&mut args, "-vn");
        }
        VideoOp::Copy => {
            push_pair(&mut args, "-map", "0:v:0");
            push_pair(&mut args, "-c:v", "copy");
        }
        VideoOp::Encode { codec, width, max_bitrate, deinterlace, tonemap } => {
            match &p.subtitle {
                SubtitleOp::BurnImage { index } => {
                    let graph =
                        build_burn_image_graph(p.backend, *deinterlace, *tonemap, *width, *index)?;
                    push_pair(&mut args, "-filter_complex", &graph);
                    push_pair(&mut args, "-map", "[vout]");
                }
                SubtitleOp::BurnText { index } => {
                    let vf = build_video_filter_chain_for_backend(
                        p.backend,
                        *deinterlace,
                        *tonemap,
                        *width,
                        Some((p.input.as_str(), *index)),
                    )?;
                    push_pair(&mut args, "-map", "0:v:0");
                    push_pair(&mut args, "-vf", &vf);
                }
                SubtitleOp::None => {
                    let vf = build_video_filter_chain_for_backend(
                        p.backend,
                        *deinterlace,
                        *tonemap,
                        *width,
                        None,
                    )?;
                    push_pair(&mut args, "-map", "0:v:0");
                    push_pair(&mut args, "-vf", &vf);
                }
            }

            push_video_encoder(
                &mut args,
                p.backend,
                *codec,
                *max_bitrate,
                p.output.segment_secs,
                caps,
            );
        }
    }

    // Audio mapping and codec options.
    push_audio_args(&mut args, &p.audio);

    // Never mux raw subtitle tracks into fMP4 segments; subtitles are either sidecar or burned in.
    push_str(&mut args, "-sn");

    // HLS fMP4 output flags.
    push_hls_output_args(&mut args, p, 0);

    Ok(args)
}

fn push_input_hwaccel(args: &mut Vec<OsString>, backend: Backend) {
    match backend {
        Backend::Software | Backend::V4l2m2m => {}
        Backend::Vaapi => {
            push_pair(args, "-hwaccel", "vaapi");
            push_pair(args, "-hwaccel_device", "/dev/dri/renderD128");
            push_pair(args, "-hwaccel_output_format", "vaapi");
        }
        Backend::Qsv => {
            push_pair(args, "-hwaccel", "qsv");
            push_pair(args, "-hwaccel_output_format", "qsv");
        }
        Backend::Nvenc => {
            push_pair(args, "-hwaccel", "cuda");
            push_pair(args, "-hwaccel_output_format", "cuda");
        }
        Backend::PiDrmDecode => {
            push_pair(args, "-hwaccel", "drm");
        }
        Backend::D3d11Amf => {
            push_pair(args, "-hwaccel", "d3d11va");
        }
    }
}

fn push_audio_args(args: &mut Vec<OsString>, audio: &AudioOp) {
    match audio {
        AudioOp::Copy { index } => {
            push_pair(args, "-map", &format!("0:{index}"));
            push_pair(args, "-c:a", "copy");
        }
        AudioOp::Encode { index, codec, channels, bitrate } => {
            push_pair(args, "-map", &format!("0:{index}"));
            push_pair(args, "-c:a", codec.ffmpeg_encoder());
            push_pair(args, "-ac", &channels.to_string());
            push_pair(args, "-b:a", &bitrate.to_string());
        }
    }
}

fn push_hls_output_args(args: &mut Vec<OsString>, p: &Pipeline, rendition_index: usize) {
    let target_dir = ffmpeg_path(p.output.rendition_dir(rendition_index).as_str());
    let segment_pattern = format!("{target_dir}/%d.m4s");
    let playlist_path = format!("{target_dir}/ffmpeg.m3u8");

    push_pair(args, "-f", "hls");
    push_pair(args, "-hls_time", &p.output.segment_secs.to_string());
    push_pair(args, "-hls_playlist_type", "event");
    push_pair(args, "-hls_segment_type", "fmp4");
    push_pair(args, "-hls_fmp4_init_filename", "init.mp4");
    push_pair(args, "-start_number", &p.output.start_number.to_string());
    push_pair(args, "-hls_segment_filename", &segment_pattern);
    push_str(args, &playlist_path);
}

fn build_abr_ladder_args(
    args: &mut Vec<OsString>,
    p: &Pipeline,
    caps: &FfmpegCaps,
    codec: VideoCodec,
    deinterlace: bool,
    tonemap: bool,
) -> Result<()> {
    let count = p.output.renditions.len();
    let mut pre_filters: Vec<String> = Vec::new();
    match p.backend {
        Backend::Vaapi => {
            if deinterlace {
                pre_filters.push("deinterlace_vaapi=rate=frame:auto=1".to_owned());
            }
            if tonemap {
                pre_filters.push(
                    "tonemap_vaapi=format=nv12:matrix=bt709:primaries=bt709:transfer=bt709"
                        .to_owned(),
                );
            }
        }
        Backend::Qsv => {
            if deinterlace || tonemap {
                let mut opts = Vec::new();
                if deinterlace {
                    opts.push("deinterlace=2");
                }
                if tonemap {
                    opts.push("tonemap=1");
                }
                opts.push("format=nv12");
                pre_filters.push(format!("vpp_qsv={}", opts.join(":")));
            }
        }
        Backend::Nvenc => {
            if deinterlace {
                pre_filters.push("yadif_cuda=mode=send_frame:parity=auto:deint=all".to_owned());
            }
            if tonemap {
                pre_filters.push("tonemap_cuda=tonemap=hable:desat=0:format=nv12".to_owned());
            }
        }
        _ => {
            if deinterlace {
                pre_filters.push("yadif=mode=send_frame:parity=auto:deint=all".to_owned());
            }
            if tonemap {
                pre_filters.push(software_tonemap_chain());
            }
        }
    }

    let split_outputs: String = (0..count).map(|i| format!("[s{i}]")).collect();
    let pre_chain = if pre_filters.is_empty() {
        format!("[0:v:0]split={count}{split_outputs}")
    } else {
        format!("[0:v:0]{},split={count}{split_outputs}", pre_filters.join(","))
    };

    let mut branches = Vec::with_capacity(count + 1);
    branches.push(pre_chain);

    for (i, r) in p.output.renditions.iter().enumerate() {
        let scale_expr = match (p.backend, r.width) {
            (Backend::Vaapi, Some(w)) => format!("scale_vaapi=w={w}:h=-2:format=nv12"),
            (Backend::Vaapi, None) => "scale_vaapi=format=nv12".to_owned(),
            (Backend::Qsv, Some(w)) => format!("vpp_qsv=w={w}:h=-2:format=nv12"),
            (Backend::Qsv, None) => "vpp_qsv=format=nv12".to_owned(),
            (Backend::Nvenc, Some(w)) => format!("scale_cuda=w={w}:h=-2:format=yuv420p"),
            (Backend::Nvenc, None) => "scale_cuda=format=yuv420p".to_owned(),
            (_, Some(w)) => format!("scale=w={w}:h=-2,format=yuv420p"),
            (_, None) => "format=yuv420p".to_owned(),
        };
        branches.push(format!("[s{i}]{scale_expr}[v{i}]"));
    }

    let filter_complex = branches.join(";");
    validate_filter_graph(&filter_complex)?;
    push_pair(args, "-filter_complex", &filter_complex);

    for (i, r) in p.output.renditions.iter().enumerate() {
        push_pair(args, "-map", &format!("[v{i}]"));
        push_video_encoder(args, p.backend, codec, r.max_bitrate, p.output.segment_secs, caps);
        push_audio_args(args, &p.audio);
        push_str(args, "-sn");
        push_hls_output_args(args, p, i);
    }

    Ok(())
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
    if !matches!(p.subtitle, SubtitleOp::None) && !matches!(p.video, VideoOp::Encode { .. }) {
        return Err(Error::Validation(
            "subtitle burn-in requires video encoding (VideoOp::Encode)".into(),
        ));
    }
    if p.output.renditions.len() > 1 {
        if !p.backend.is_gpu() {
            return Err(Error::Validation(format!(
                "multi-rendition ABR ladder requires a GPU backend, not {:?}",
                p.backend
            )));
        }
        if !matches!(p.video, VideoOp::Encode { .. }) {
            return Err(Error::Validation(
                "multi-rendition ABR ladder requires VideoOp::Encode".into(),
            ));
        }
        for r in &p.output.renditions {
            if r.name.is_empty() {
                return Err(Error::Validation("ABR rendition name must not be empty".into()));
            }
            if r.max_bitrate == 0 {
                return Err(Error::Validation("ABR rendition max_bitrate must be > 0".into()));
            }
            if matches!(r.width, Some(0)) {
                return Err(Error::Validation(
                    "ABR rendition width must be > 0 when specified".into(),
                ));
            }
        }
    }
    if let VideoOp::Encode { codec, width, max_bitrate, tonemap, .. } = &p.video {
        if *max_bitrate == 0 {
            return Err(Error::Validation("video max_bitrate must be > 0".into()));
        }
        if matches!(width, Some(0)) {
            return Err(Error::Validation("video width must be > 0 when specified".into()));
        }
        if p.backend.is_pi() && *tonemap {
            return Err(Error::Validation(
                "HDR tone mapping is disabled on Raspberry Pi profiles".into(),
            ));
        }
        if p.backend == Backend::V4l2m2m && *codec != VideoCodec::H264 {
            return Err(Error::Validation(
                "v4l2m2m backend only supports H.264 video encoding".into(),
            ));
        }
        if p.backend == Backend::PiDrmDecode && *codec != VideoCodec::H264 {
            return Err(Error::Validation(
                "piDrmDecode backend only supports software H.264 video encoding on Pi 5".into(),
            ));
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

fn software_tonemap_chain() -> String {
    "zscale=t=linear:npl=100,format=gbrpf32le,zscale=p=bt709,tonemap=tonemap=hable:desat=0,zscale=t=bt709:m=bt709:r=tv"
        .to_owned()
}

fn build_video_filter_chain_for_backend(
    backend: Backend,
    deinterlace: bool,
    tonemap: bool,
    width: Option<u32>,
    burn_text: Option<(&str, u32)>,
) -> Result<String> {
    let mut filters: Vec<String> = Vec::with_capacity(6);
    match backend {
        Backend::Vaapi => {
            if deinterlace {
                filters.push("deinterlace_vaapi=rate=frame:auto=1".to_owned());
            }
            if tonemap {
                filters.push(
                    "tonemap_vaapi=format=nv12:matrix=bt709:primaries=bt709:transfer=bt709"
                        .to_owned(),
                );
            }
            if let Some(w) = width {
                filters.push(format!("scale_vaapi=w={w}:h=-2:format=nv12"));
            } else if !tonemap {
                filters.push("scale_vaapi=format=nv12".to_owned());
            }
            if let Some((input_path, sub_index)) = burn_text {
                let escaped = escape_filter_path(input_path)?;
                filters.push(format!(
                    "hwdownload,format=nv12,subtitles=filename={escaped}:si={sub_index},hwupload"
                ));
            }
        }
        Backend::Qsv => {
            let mut opts: Vec<String> = Vec::new();
            if deinterlace {
                opts.push("deinterlace=2".to_owned());
            }
            if tonemap {
                opts.push("tonemap=1".to_owned());
            }
            if let Some(w) = width {
                opts.push(format!("w={w}:h=-2"));
            }
            opts.push("format=nv12".to_owned());
            filters.push(format!("vpp_qsv={}", opts.join(":")));

            if let Some((input_path, sub_index)) = burn_text {
                let escaped = escape_filter_path(input_path)?;
                filters.push(format!(
                    "hwdownload,format=nv12,subtitles=filename={escaped}:si={sub_index},hwupload=extra_hw_frames=64,format=qsv"
                ));
            }
        }
        Backend::Nvenc => {
            if deinterlace {
                filters.push("yadif_cuda=mode=send_frame:parity=auto:deint=all".to_owned());
            }
            if tonemap {
                filters.push("tonemap_cuda=tonemap=hable:desat=0:format=nv12".to_owned());
            }
            if let Some(w) = width {
                let fmt = if tonemap { "nv12" } else { "yuv420p" };
                filters.push(format!("scale_cuda=w={w}:h=-2:format={fmt}"));
            } else if !tonemap {
                filters.push("scale_cuda=format=yuv420p".to_owned());
            }
            if let Some((input_path, sub_index)) = burn_text {
                let escaped = escape_filter_path(input_path)?;
                let fmt = if tonemap { "nv12" } else { "yuv420p" };
                filters.push(format!(
                    "hwdownload,format={fmt},subtitles=filename={escaped}:si={sub_index},hwupload_cuda"
                ));
            }
        }
        Backend::Software | Backend::V4l2m2m | Backend::PiDrmDecode | Backend::D3d11Amf => {
            if deinterlace {
                filters.push("yadif=mode=send_frame:parity=auto:deint=all".to_owned());
            }
            if tonemap {
                filters.push(software_tonemap_chain());
            }
            if let Some(w) = width {
                filters.push(format!("scale=w={w}:h=-2"));
            }
            if let Some((input_path, sub_index)) = burn_text {
                let escaped = escape_filter_path(input_path)?;
                filters.push(format!("subtitles=filename={escaped}:si={sub_index}"));
            }
            filters.push("format=yuv420p".to_owned());
        }
    }

    let chain = filters.join(",");
    validate_filter_graph(&chain)?;
    Ok(chain)
}

fn build_burn_image_graph(
    backend: Backend,
    deinterlace: bool,
    tonemap: bool,
    width: Option<u32>,
    subtitle_stream_index: u32,
) -> Result<String> {
    let mut tail = String::from("overlay=eof_action=pass");
    if let Some(w) = width {
        tail.push_str(&format!(",scale=w={w}:h=-2"));
    }
    match backend {
        Backend::Vaapi => {
            tail.push_str(",format=nv12,hwupload[vout]");
            let mut pre = Vec::new();
            if deinterlace {
                pre.push("deinterlace_vaapi=rate=frame:auto=1");
            }
            if tonemap {
                pre.push("tonemap_vaapi=format=nv12:matrix=bt709:primaries=bt709:transfer=bt709");
            }
            pre.push("hwdownload,format=nv12");
            let graph =
                format!("[0:v:0]{}[v0];[v0][0:{subtitle_stream_index}]{tail}", pre.join(","));
            validate_filter_graph(&graph)?;
            Ok(graph)
        }
        _ => {
            tail.push_str(",format=yuv420p[vout]");
            let mut pre = Vec::new();
            if deinterlace {
                pre.push("yadif=mode=send_frame:parity=auto:deint=all".to_owned());
            }
            if tonemap {
                pre.push(software_tonemap_chain());
            }
            let graph = if pre.is_empty() {
                format!("[0:v:0][0:{subtitle_stream_index}]{tail}")
            } else {
                format!("[0:v:0]{}[v0];[v0][0:{subtitle_stream_index}]{tail}", pre.join(","))
            };
            validate_filter_graph(&graph)?;
            Ok(graph)
        }
    }
}

fn push_video_encoder(
    args: &mut Vec<OsString>,
    backend: Backend,
    codec: VideoCodec,
    max_bitrate: u64,
    segment_secs: u32,
    caps: &FfmpegCaps,
) {
    let bufsize = max_bitrate.saturating_mul(2);
    match backend {
        Backend::Software | Backend::PiDrmDecode => {
            push_software_video_encoder(args, codec, max_bitrate, segment_secs, caps);
            return;
        }
        Backend::Vaapi => {
            match codec {
                VideoCodec::H264 => push_pair(args, "-c:v", "h264_vaapi"),
                VideoCodec::Hevc => {
                    push_pair(args, "-c:v", "hevc_vaapi");
                    push_pair(args, "-tag:v", "hvc1");
                }
                VideoCodec::Av1 => push_pair(args, "-c:v", "av1_vaapi"),
            }
            push_pair(args, "-b:v", &max_bitrate.to_string());
            push_pair(args, "-maxrate", &max_bitrate.to_string());
            push_pair(args, "-bufsize", &bufsize.to_string());
        }
        Backend::Qsv => {
            match codec {
                VideoCodec::H264 => push_pair(args, "-c:v", "h264_qsv"),
                VideoCodec::Hevc => {
                    push_pair(args, "-c:v", "hevc_qsv");
                    push_pair(args, "-tag:v", "hvc1");
                }
                VideoCodec::Av1 => push_pair(args, "-c:v", "av1_qsv"),
            }
            push_pair(args, "-low_power", "1");
            push_pair(args, "-b:v", &max_bitrate.to_string());
            push_pair(args, "-maxrate", &max_bitrate.to_string());
            push_pair(args, "-bufsize", &bufsize.to_string());
        }
        Backend::Nvenc => {
            match codec {
                VideoCodec::H264 => push_pair(args, "-c:v", "h264_nvenc"),
                VideoCodec::Hevc => {
                    push_pair(args, "-c:v", "hevc_nvenc");
                    push_pair(args, "-tag:v", "hvc1");
                }
                VideoCodec::Av1 => push_pair(args, "-c:v", "av1_nvenc"),
            }
            push_pair(args, "-preset", "p4");
            push_pair(args, "-rc", "vbr");
            push_pair(args, "-cq", &caps.crf.to_string());
            push_pair(args, "-b:v", &max_bitrate.to_string());
            push_pair(args, "-maxrate", &max_bitrate.to_string());
            push_pair(args, "-bufsize", &bufsize.to_string());
        }
        Backend::V4l2m2m => {
            push_pair(args, "-c:v", "h264_v4l2m2m");
            push_pair(args, "-num_output_buffers", "32");
            push_pair(args, "-num_capture_buffers", "16");
            push_pair(args, "-b:v", &max_bitrate.to_string());
            push_pair(args, "-maxrate", &max_bitrate.to_string());
            push_pair(args, "-bufsize", &bufsize.to_string());
            push_pair(args, "-pix_fmt", "yuv420p");
        }
        Backend::D3d11Amf => {
            match codec {
                VideoCodec::H264 => push_pair(args, "-c:v", "h264_amf"),
                VideoCodec::Hevc => {
                    push_pair(args, "-c:v", "hevc_amf");
                    push_pair(args, "-tag:v", "hvc1");
                }
                VideoCodec::Av1 => push_pair(args, "-c:v", "av1_amf"),
            }
            push_pair(args, "-quality", "balanced");
            push_pair(args, "-rc", "vbr_peak");
            push_pair(args, "-b:v", &max_bitrate.to_string());
            push_pair(args, "-maxrate", &max_bitrate.to_string());
            push_pair(args, "-bufsize", &bufsize.to_string());
        }
    }
    push_pair(args, "-force_key_frames", &format!("expr:gte(t,n_forced*{segment_secs})"));
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
