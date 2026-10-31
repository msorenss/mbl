//! Public HTTP fetches: validate every redirect and pin validated DNS answers.
use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use async_trait::async_trait;
use mbl_core::{Error, Result};
use reqwest::{Url, header};

#[derive(Debug)]
pub(super) struct Fetched {
    pub url: Url,
    pub status: u16,
    pub content_type: String,
    pub body: Vec<u8>,
}

#[async_trait]
pub(super) trait OnlineHttp: std::fmt::Debug + Send + Sync {
    async fn fetch(&self, url: Url, limit: usize, prefix: bool) -> Result<Fetched>;
}

#[derive(Debug)]
pub(super) struct PublicHttp;

pub(super) fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let [a, b, c, _] = v4.octets();
            !(v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.is_multicast()
                || a == 0
                || a >= 224
                || (a == 100 && (64..=127).contains(&b))
                || (a == 192 && b == 0 && (c == 0 || c == 2))
                || (a == 192 && b == 88 && c == 99)
                || (a == 198 && (b == 18 || b == 19 || (b == 51 && c == 100)))
                || (a == 203 && b == 0 && c == 113))
        }
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return public_ip(IpAddr::V4(v4));
            }
            let s = v6.segments();
            (s[0] & 0xe000) == 0x2000
                && s[0] != 0x2002
                && !(s[0] == 0x2001 && (s[1] < 0x0200 || s[1] == 0x0db8))
        }
    }
}

pub(super) fn public_url(raw: &str) -> Result<Url> {
    if raw.len() > 4096 {
        return Err(Error::Validation("Source URL is too long".into()));
    }
    let mut url = Url::parse(raw)
        .map_err(|_| Error::Validation("A public HTTP(S) source URL is required".into()))?;
    let port = match url.scheme() {
        "https" => 443,
        "http" => 80,
        _ => return Err(Error::Validation("Only public HTTP(S) source URLs are allowed".into())),
    };
    if !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some_and(|p| p != port)
    {
        return Err(Error::Validation(
            "Source URLs must not contain credentials or custom ports".into(),
        ));
    }
    let host = url.host_str().ok_or_else(|| Error::Validation("Source URL needs a host".into()))?;
    let bare = host.trim_matches(['[', ']']).trim_end_matches('.').to_ascii_lowercase();
    if let Ok(ip) = bare.parse::<IpAddr>() {
        if !public_ip(ip) {
            return Err(Error::Validation(
                "Private or reserved source addresses are not allowed".into(),
            ));
        }
    } else {
        if !bare.contains('.')
            || bare == "localhost"
            || bare.ends_with(".localhost")
            || bare.ends_with(".local")
            || bare.ends_with(".internal")
        {
            return Err(Error::Validation("A public internet hostname is required".into()));
        }
        // The resolver override and the HTTP connector must use the identical name.
        url.set_host(Some(&bare))
            .map_err(|_| Error::Validation("Invalid source hostname".into()))?;
    }
    url.set_fragment(None);
    if url.as_str().len() > 4096 {
        return Err(Error::Validation("Source URL is too long".into()));
    }
    Ok(url)
}

#[async_trait]
impl OnlineHttp for PublicHttp {
    async fn fetch(&self, url: Url, limit: usize, prefix: bool) -> Result<Fetched> {
        let mut url = public_url(url.as_str())?;
        for redirects in 0..=3 {
            let host = url
                .host_str()
                .ok_or_else(|| Error::Validation("Source URL needs a host".into()))?;
            let port = url.port_or_known_default().unwrap_or(443);
            let addresses: Vec<SocketAddr> =
                tokio::net::lookup_host((host.trim_matches(['[', ']']), port))
                    .await
                    .map_err(|_| {
                        Error::Unavailable("The source hostname could not be resolved".into())
                    })?
                    .take(17)
                    .collect();
            if addresses.is_empty()
                || addresses.len() > 16
                || addresses.iter().any(|a| !public_ip(a.ip()))
            {
                return Err(Error::Validation(
                    "Source resolves to a private or reserved address".into(),
                ));
            }
            let client = reqwest::Client::builder()
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .referer(false)
                .timeout(Duration::from_secs(8))
                .connect_timeout(Duration::from_secs(4))
                .user_agent("MBL/1.1 public-source-discovery")
                .resolve_to_addrs(host, &addresses)
                .build()
                .map_err(Error::internal)?;
            let mut request = client.get(url.clone());
            if prefix {
                request =
                    request.header(header::RANGE, format!("bytes=0-{}", limit.saturating_sub(1)));
            }
            let mut response = request
                .send()
                .await
                .map_err(|_| Error::Unavailable("The public source did not respond".into()))?;
            if response.status().is_redirection() {
                if redirects == 3 {
                    return Err(Error::Unavailable("Too many source redirects".into()));
                }
                let location = response
                    .headers()
                    .get(header::LOCATION)
                    .and_then(|v| v.to_str().ok())
                    .ok_or_else(|| {
                        Error::Unavailable("Source redirect has no valid destination".into())
                    })?;
                url = public_url(
                    url.join(location)
                        .map_err(|_| Error::Validation("Invalid source redirect".into()))?
                        .as_str(),
                )?;
                continue;
            }
            let status = response.status().as_u16();
            let content_type = response
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .split(';')
                .next()
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase();
            let mut body = Vec::new();
            let cap = limit.saturating_add(usize::from(!prefix));
            while body.len() < cap {
                let Some(chunk) = response
                    .chunk()
                    .await
                    .map_err(|_| Error::Unavailable("Source transfer failed".into()))?
                else {
                    break;
                };
                let take = chunk.len().min(cap - body.len());
                body.extend_from_slice(&chunk[..take]);
            }
            if body.len() > limit {
                return Err(Error::Unavailable("Source exceeds transfer limit".into()));
            }
            return Ok(Fetched { url, status, content_type, body });
        }
        Err(Error::Unavailable("Source redirect limit exceeded".into()))
    }
}
