//! Read dimensions and the RFC 6381 codec from a generated AVC init segment.
//! Walk only the ISO BMFF containers needed for a video sample entry.
pub fn avc_init_metadata(data: &[u8]) -> Option<(u32, u32, String)> {
    walk(data, 0)
}
fn walk(mut data: &[u8], depth: u8) -> Option<(u32, u32, String)> {
    if depth > 10 {
        return None;
    }
    while data.len() >= 8 {
        let size = u32::from_be_bytes(data[..4].try_into().ok()?) as usize;
        if size < 8 || size > data.len() {
            return None;
        }
        let kind = &data[4..8];
        let body = &data[8..size];
        if kind == b"avc1" || kind == b"avc3" {
            if body.len() < 78 {
                return None;
            }
            let width = u16::from_be_bytes(body[24..26].try_into().ok()?) as u32;
            let height = u16::from_be_bytes(body[26..28].try_into().ok()?) as u32;
            let mut children = &body[78..];
            while children.len() >= 8 {
                let n = u32::from_be_bytes(children[..4].try_into().ok()?) as usize;
                if n < 8 || n > children.len() {
                    return None;
                }
                if &children[4..8] == b"avcC"
                    && n >= 12
                    && children[8] == 1
                    && width > 0
                    && height > 0
                {
                    return Some((
                        width,
                        height,
                        format!(
                            "{}.{:02x}{:02x}{:02x}",
                            std::str::from_utf8(kind).ok()?,
                            children[9],
                            children[10],
                            children[11]
                        ),
                    ));
                }
                children = &children[n..];
            }
        } else {
            let nested = match kind {
                b"moov" | b"trak" | b"mdia" | b"minf" | b"stbl" => Some(body),
                b"stsd" => body.get(8..),
                _ => None,
            };
            if let Some(found) = nested.and_then(|b| walk(b, depth + 1)) {
                return Some(found);
            }
        }
        data = &data[size..];
    }
    None
}
