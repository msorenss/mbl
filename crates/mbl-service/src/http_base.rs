//! The server's own absolute base URL as seen by a client, for OAuth metadata and challenges.

use axum::http::{Extensions, HeaderMap, header};

/// Marks a request that arrived on the server's own HTTPS listener. The listener adds it as a
/// request extension; the plain HTTP listener never does.
#[derive(Debug, Clone, Copy)]
pub struct SecureTransport;

/// Absolute base URL (`scheme://host[:port]`, no trailing slash) for links handed to clients.
///
/// A configured public URL wins. Otherwise the request's `Host` header is used with `https` when
/// the request came in on the built-in HTTPS listener. Client headers such as
/// `X-Forwarded-Proto` are not trusted. `None` when no usable host is known.
pub fn request_base_url(
    headers: &HeaderMap,
    extensions: &Extensions,
    public_url: &str,
) -> Option<String> {
    let public_url = public_url.trim().trim_end_matches('/');
    if public_url.starts_with("http://") || public_url.starts_with("https://") {
        return Some(public_url.to_owned());
    }
    let host = headers.get(header::HOST)?.to_str().ok()?.trim();
    let valid = !host.is_empty()
        && host.len() <= 255
        && host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b':' | b'[' | b']'));
    if !valid {
        return None;
    }
    let scheme = if extensions.get::<SecureTransport>().is_some() { "https" } else { "http" };
    Some(format!("{scheme}://{host}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn prefers_public_url_then_host() {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, HeaderValue::from_static("192.168.1.5:8420"));
        let mut ext = Extensions::new();
        assert_eq!(
            request_base_url(&headers, &ext, "").as_deref(),
            Some("http://192.168.1.5:8420")
        );
        ext.insert(SecureTransport);
        assert_eq!(
            request_base_url(&headers, &ext, "").as_deref(),
            Some("https://192.168.1.5:8420")
        );
        assert_eq!(
            request_base_url(&headers, &ext, "https://mbl.example.com/").as_deref(),
            Some("https://mbl.example.com")
        );
        headers.insert(header::HOST, HeaderValue::from_static("evil.com/\"x"));
        assert_eq!(request_base_url(&headers, &ext, ""), None);
    }
}
