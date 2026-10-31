//! Bounded VOD HLS parsing. Every URL becomes an opaque authenticated relay resource.
use super::*;

#[derive(Debug, Clone)]
pub(super) struct Playlist {
    pub lines: Vec<String>,
    pub urls: Vec<String>,
    pub init: Option<usize>,
    pub first: usize,
    pub duration: f64,
}
fn invalid() -> Error {
    Error::Validation("Unsupported HLS: use unencrypted VOD with TS/fMP4 segments, without external tracks, ranges or low-latency extensions".into())
}
fn link(base: &Url, raw: &str) -> Result<String> {
    Ok(public_url(base.join(raw).map_err(|_| invalid())?.as_str())?.to_string())
}
pub(super) fn variant(base: &Url, body: &str) -> Result<Option<String>> {
    if !body.trim_start().starts_with("#EXTM3U") || body.len() > 512 * 1024 {
        return Err(invalid());
    }
    let mut pending = false;
    let mut selected = None;
    for line in body.lines().map(str::trim) {
        if line.starts_with("#EXT-X-MEDIA:") || line.starts_with("#EXT-X-SESSION-KEY:") {
            return Err(invalid());
        }
        if line.starts_with("#EXT-X-STREAM-INF:") {
            pending = true;
            continue;
        }
        if pending && !line.is_empty() && !line.starts_with('#') {
            if selected.is_none() {
                selected = Some(link(base, line)?);
            }
            pending = false;
        }
    }
    if pending { Err(invalid()) } else { Ok(selected) }
}
pub(super) fn parse(base: &Url, body: &str) -> Result<Playlist> {
    if variant(base, body)?.is_some() {
        return Err(invalid());
    }
    let mut out =
        Playlist { lines: Vec::new(), urls: Vec::new(), init: None, first: 0, duration: 0.0 };
    let mut ended = false;
    let mut pending = false;
    let mut segments = 0;
    for line in body.lines().map(str::trim).filter(|s| !s.is_empty()).take(8193) {
        if out.lines.len() >= 8192 || out.urls.len() >= 2048 {
            return Err(invalid());
        }
        if line == "#EXT-X-ENDLIST" {
            ended = true;
            out.lines.push(line.into());
            continue;
        }
        if ended {
            return Err(invalid());
        }
        if let Some(value) = line.strip_prefix("#EXTINF:") {
            if pending {
                return Err(invalid());
            }
            let seconds =
                value.split(',').next().unwrap_or("").parse::<f64>().map_err(|_| invalid())?;
            if !seconds.is_finite() || !(0.001..=120.0).contains(&seconds) {
                return Err(invalid());
            }
            out.duration += seconds;
            pending = true;
            out.lines.push(format!("#EXTINF:{seconds},"));
            continue;
        }
        if let Some(value) = line.strip_prefix("#EXT-X-MAP:URI=\"") {
            if segments > 0 || out.init.is_some() {
                return Err(invalid());
            }
            let raw = value.strip_suffix('"').ok_or_else(invalid)?;
            if raw.contains('"') {
                return Err(invalid());
            }
            let index = out.urls.len();
            out.urls.push(link(base, raw)?);
            out.init = Some(index);
            out.lines.push(format!("#EXT-X-MAP:URI=\"resource/{index}\""));
            continue;
        }
        if line.starts_with('#') {
            let allowed = line == "#EXTM3U"
                || line == "#EXT-X-DISCONTINUITY"
                || line == "#EXT-X-INDEPENDENT-SEGMENTS"
                || line == "#EXT-X-PLAYLIST-TYPE:VOD"
                || [
                    "#EXT-X-VERSION:",
                    "#EXT-X-TARGETDURATION:",
                    "#EXT-X-MEDIA-SEQUENCE:",
                    "#EXT-X-DISCONTINUITY-SEQUENCE:",
                ]
                .iter()
                .any(|tag| line.strip_prefix(tag).is_some_and(|n| n.parse::<u32>().is_ok()));
            if !allowed {
                return Err(invalid());
            }
            out.lines.push(line.into());
            continue;
        }
        if !pending {
            return Err(invalid());
        }
        let index = out.urls.len();
        out.urls.push(link(base, line)?);
        if segments == 0 {
            out.first = index;
        }
        segments += 1;
        pending = false;
        out.lines.push(format!("resource/{index}"));
    }
    if !ended || pending || segments == 0 || out.duration > 24.0 * 3600.0 {
        return Err(invalid());
    }
    Ok(out)
}
