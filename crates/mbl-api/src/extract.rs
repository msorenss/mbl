//! Request extractors: authentication (bearer header or session cookie) and client info.

use std::net::SocketAddr;

use axum::extract::{ConnectInfo, FromRequestParts};
use axum::http::request::Parts;
use axum::http::{HeaderMap, Method, header};
use mbl_core::Error;
use mbl_service::{Authenticated, DeviceInfo};

use crate::{ApiError, AppState};

pub const SESSION_COOKIE: &str = "mbl_session";

/// Any authenticated user.
#[derive(Debug, Clone)]
pub struct AuthUser(pub Authenticated);

/// An authenticated admin.
#[derive(Debug, Clone)]
pub struct AdminUser(pub Authenticated);

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, ApiError> {
        let token = if let Some(token) = bearer_token(&parts.headers) {
            token
        } else if let Some(token) = cookie(&parts.headers, SESSION_COOKIE) {
            // Cookies are sent automatically by browsers: reject cross-site state changes.
            // WebSocket upgrades are GETs but carry the session too (cross-site hijacking).
            if (is_unsafe(&parts.method) || is_upgrade(&parts.headers))
                && !same_origin(&parts.headers)
            {
                return Err(Error::Forbidden.into());
            }
            token
        } else {
            return Err(Error::Unauthorized.into());
        };
        Ok(Self(state.services.auth.authenticate(&token).await?))
    }
}

/// Request network classification (LAN, Tailnet, WAN).
#[derive(Debug, Clone, Copy)]
pub struct ClientNetwork(pub mbl_remote::NetworkClass);

impl FromRequestParts<AppState> for ClientNetwork {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &AppState) -> Result<Self, ApiError> {
        let peer_addr = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map(|ci| ci.0)
            .unwrap_or_else(|| SocketAddr::from(([127, 0, 0, 1], 0)));
        let fwd_header = parts.headers.get("x-forwarded-for").and_then(|v| v.to_str().ok());
        let client_ip = mbl_remote::resolve_client_ip(peer_addr, fwd_header, &[]);

        Ok(Self(mbl_remote::classify_ip(client_ip)))
    }
}

impl FromRequestParts<AppState> for AdminUser {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, ApiError> {
        let AuthUser(auth) = AuthUser::from_request_parts(parts, state).await?;
        auth.actor.require_admin()?;

        // M6.12: Admin endpoints LAN/tailnet-only by default
        let ClientNetwork(net) = ClientNetwork::from_request_parts(parts, state).await?;
        if net.is_wan() {
            let admin_lan_only = mbl_db::repo::m6::get_remote_setting(&state.db, "admin_lan_only")
                .await
                .ok()
                .flatten()
                .map(|s| s == "true" || s == "1")
                .unwrap_or(true);
            if admin_lan_only {
                return Err(Error::Forbidden.into());
            }
        }

        Ok(Self(auth))
    }
}

/// Best-effort client address for rate limiting; "unknown" when not served over TCP (tests).
pub fn client_ip(parts_or_extensions: &axum::http::Extensions) -> String {
    parts_or_extensions
        .get::<ConnectInfo<SocketAddr>>()
        .map_or_else(|| "unknown".to_owned(), |ci| ci.0.ip().to_string())
}

pub fn device_info(
    headers: &HeaderMap,
    device_name: Option<String>,
    client_name: Option<String>,
) -> DeviceInfo {
    let user_agent = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(|ua| ua.chars().take(200).collect::<String>());
    DeviceInfo {
        device_name: device_name.map(|d| d.chars().take(100).collect()),
        client_name: client_name.map(|c| c.chars().take(100).collect()).or(user_agent),
    }
}

pub(crate) fn bearer_token(headers: &HeaderMap) -> Option<String> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = value.split_once(' ')?;
    (scheme.eq_ignore_ascii_case("bearer") && !token.trim().is_empty())
        .then(|| token.trim().to_owned())
}

pub(crate) fn cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(k, _)| *k == name)
        .map(|(_, v)| v.to_owned())
        .filter(|v| !v.is_empty())
}

fn is_unsafe(method: &Method) -> bool {
    !matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS)
}

fn is_upgrade(headers: &HeaderMap) -> bool {
    headers.contains_key(header::UPGRADE)
}

/// True when there is no `Origin` header or its host matches the `Host` header.
fn same_origin(headers: &HeaderMap) -> bool {
    let Some(origin) = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) else {
        return true;
    };
    let origin_host = origin.split_once("://").map_or(origin, |(_, rest)| rest);
    let host = headers.get(header::HOST).and_then(|v| v.to_str().ok());
    host.is_some_and(|h| h.eq_ignore_ascii_case(origin_host))
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;

    fn headers(pairs: &[(header::HeaderName, &str)]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for (k, v) in pairs {
            h.append(k.clone(), HeaderValue::from_str(v).unwrap());
        }
        h
    }

    #[test]
    fn parses_bearer() {
        assert_eq!(
            bearer_token(&headers(&[(header::AUTHORIZATION, "Bearer abc")])).as_deref(),
            Some("abc")
        );
        assert_eq!(
            bearer_token(&headers(&[(header::AUTHORIZATION, "bearer  abc ")])).as_deref(),
            Some("abc")
        );
        assert_eq!(bearer_token(&headers(&[(header::AUTHORIZATION, "Basic abc")])), None);
        assert_eq!(bearer_token(&headers(&[(header::AUTHORIZATION, "Bearer ")])), None);
    }

    #[test]
    fn parses_cookie() {
        let h = headers(&[(header::COOKIE, "a=1; mbl_session=tok; b=2")]);
        assert_eq!(cookie(&h, SESSION_COOKIE).as_deref(), Some("tok"));
        assert_eq!(cookie(&headers(&[(header::COOKIE, "mbl_session=")]), SESSION_COOKIE), None);
    }

    #[test]
    fn origin_check() {
        assert!(same_origin(&headers(&[(header::HOST, "mbl.local:8420")])));
        assert!(same_origin(&headers(&[
            (header::HOST, "mbl.local:8420"),
            (header::ORIGIN, "http://mbl.local:8420"),
        ])));
        assert!(!same_origin(&headers(&[
            (header::HOST, "mbl.local:8420"),
            (header::ORIGIN, "https://evil.example"),
        ])));
    }
}
