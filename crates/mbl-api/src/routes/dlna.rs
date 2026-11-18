//! DLNA/UPnP media server over HTTP: descriptions, ContentDirectory control and media.
//!
//! DLNA renderers cannot sign in. Every route answers only clients on the local network
//! (by peer address; forwarding headers are ignored) and only while DLNA is turned on with at
//! least one shared library. Media access is limited to that shared scope.

use std::net::{IpAddr, SocketAddr};

use axum::Router;
use axum::extract::{ConnectInfo, DefaultBodyLimit, Path, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{any, get, post};
use mbl_core::{Error, PartId};
use mbl_remote::dlna::{self, BrowseFlag, DlnaRendererProfile, escape_xml};
use mbl_remote::{NetworkClass, classify_ip};
use mbl_service::dlna::Renderer;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/dlna/description.xml", get(description))
        .route("/dlna/content_directory.xml", get(content_directory_scpd))
        .route("/dlna/connection_manager.xml", get(connection_manager_scpd))
        .route("/dlna/control", post(content_directory_control))
        .route("/dlna/connection_control", post(connection_manager_control))
        .route("/dlna/events", any(events))
        .route("/dlna/connection_events", any(events))
        .route("/dlna/media/{file}", get(media).head(media))
        .layer(DefaultBodyLimit::max(64 * 1024))
}

/// The connecting peer's address. Without connection info (in-process tests) it is loopback.
struct Peer(IpAddr);

impl<S: Send + Sync> axum::extract::FromRequestParts<S> for Peer {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let ip = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map_or(IpAddr::from([127, 0, 0, 1]), |ConnectInfo(addr)| addr.ip());
        Ok(Self(ip))
    }
}

const XML: &str = "text/xml; charset=\"utf-8\"";

/// Gate shared by all routes: LAN peer and DLNA active.
async fn guard(state: &AppState, peer: IpAddr) -> Result<(), StatusCode> {
    if classify_ip(peer) != NetworkClass::Lan {
        return Err(StatusCode::FORBIDDEN);
    }
    match state.services.dlna.settings().await {
        Ok(settings) if settings.is_active() => Ok(()),
        Ok(_) => Err(StatusCode::NOT_FOUND),
        Err(error) => {
            tracing::warn!(%error, "DLNA settings unavailable");
            Err(StatusCode::SERVICE_UNAVAILABLE)
        }
    }
}

fn xml(status: StatusCode, body: String) -> Response {
    (status, [(header::CONTENT_TYPE, XML)], body).into_response()
}

fn profile(headers: &HeaderMap) -> DlnaRendererProfile {
    let text = |name: &str| headers.get(name).and_then(|v| v.to_str().ok()).unwrap_or("");
    DlnaRendererProfile::from_user_agent(&format!(
        "{} {}",
        text("user-agent"),
        text("x-av-client-info")
    ))
}

async fn description(State(state): State<AppState>, Peer(peer): Peer) -> Response {
    if let Err(status) = guard(&state, peer).await {
        return status.into_response();
    }
    let dlna = &state.services.dlna;
    match (dlna.settings().await, dlna.udn().await) {
        (Ok(settings), Ok(udn)) => xml(
            StatusCode::OK,
            dlna::device_description(&settings.friendly_name, &udn, env!("CARGO_PKG_VERSION")),
        ),
        _ => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}

async fn content_directory_scpd(State(state): State<AppState>, Peer(peer): Peer) -> Response {
    match guard(&state, peer).await {
        Ok(()) => xml(StatusCode::OK, dlna::content_directory_scpd()),
        Err(status) => status.into_response(),
    }
}

async fn connection_manager_scpd(State(state): State<AppState>, Peer(peer): Peer) -> Response {
    match guard(&state, peer).await {
        Ok(()) => xml(StatusCode::OK, dlna::connection_manager_scpd()),
        Err(status) => status.into_response(),
    }
}

fn fault(code: u16, description: &str) -> Response {
    xml(StatusCode::INTERNAL_SERVER_ERROR, dlna::soap_fault(code, description))
}

fn action(headers: &HeaderMap) -> Option<String> {
    headers
        .get("soapaction")
        .and_then(|v| v.to_str().ok())
        .and_then(dlna::soap_action)
        .map(str::to_owned)
}

/// `http://host[:port]` as the renderer addressed us, when the Host header is plausible.
fn base_url(headers: &HeaderMap) -> Option<String> {
    let host = headers.get(header::HOST)?.to_str().ok()?.trim();
    let valid = !host.is_empty()
        && host.len() <= 255
        && host.bytes().all(|b| b.is_ascii_alphanumeric() || b".-:[]_".contains(&b));
    valid.then(|| format!("http://{host}"))
}

async fn content_directory_control(
    State(state): State<AppState>,
    Peer(peer): Peer,
    headers: HeaderMap,
    body: String,
) -> Response {
    if let Err(status) = guard(&state, peer).await {
        return status.into_response();
    }
    let service = dlna::CONTENT_DIRECTORY;
    let respond = |name: &str, args: &[(&str, String)]| {
        xml(StatusCode::OK, dlna::soap_response(service, name, args))
    };
    match action(&headers).as_deref() {
        Some("GetSearchCapabilities") => {
            respond("GetSearchCapabilities", &[("SearchCaps", String::new())])
        }
        Some("GetSortCapabilities") => {
            respond("GetSortCapabilities", &[("SortCaps", String::new())])
        }
        Some("GetSystemUpdateID") => respond("GetSystemUpdateID", &[("Id", "1".into())]),
        Some("Browse") => {
            let arg = |name| dlna::soap_arg(&body, name);
            let number = |name| arg(name).map_or(Some(0), |v| v.trim().parse::<u32>().ok());
            let (Some(object), Some(flag), Some(start), Some(count), Some(base)) = (
                arg("ObjectID"),
                arg("BrowseFlag").as_deref().and_then(BrowseFlag::parse),
                number("StartingIndex"),
                number("RequestedCount"),
                base_url(&headers),
            ) else {
                return fault(402, "Invalid Args");
            };
            let renderer = Renderer { base_url: &base, profile: profile(&headers) };
            match state.services.dlna.browse(&object, flag, start, count, renderer).await {
                Ok(page) => respond(
                    "Browse",
                    &[
                        ("Result", escape_xml(&dlna::didl_lite(&page.items))),
                        ("NumberReturned", page.items.len().to_string()),
                        ("TotalMatches", page.total.to_string()),
                        ("UpdateID", "1".into()),
                    ],
                ),
                Err(Error::NotFound) => fault(701, "No such object"),
                Err(Error::Validation(_)) => fault(402, "Invalid Args"),
                Err(error) => {
                    tracing::warn!(%error, "DLNA browse failed");
                    fault(501, "Action Failed")
                }
            }
        }
        _ => fault(401, "Invalid Action"),
    }
}

async fn connection_manager_control(
    State(state): State<AppState>,
    Peer(peer): Peer,
    headers: HeaderMap,
) -> Response {
    if let Err(status) = guard(&state, peer).await {
        return status.into_response();
    }
    let service = dlna::CONNECTION_MANAGER;
    match action(&headers).as_deref() {
        Some("GetProtocolInfo") => {
            let source = [
                "video/mp4",
                "video/x-matroska",
                "video/x-mkv",
                "video/x-msvideo",
                "video/mpeg",
                "video/vnd.dlna.mpeg-tts",
                "video/webm",
                "audio/mpeg",
                "audio/mp4",
                "audio/flac",
                "image/jpeg",
                "image/png",
            ]
            .map(dlna::protocol_info)
            .join(",");
            let body = dlna::soap_response(
                service,
                "GetProtocolInfo",
                &[("Source", escape_xml(&source)), ("Sink", String::new())],
            );
            xml(StatusCode::OK, body)
        }
        Some("GetCurrentConnectionIDs") => xml(
            StatusCode::OK,
            dlna::soap_response(
                service,
                "GetCurrentConnectionIDs",
                &[("ConnectionIDs", "0".into())],
            ),
        ),
        _ => fault(401, "Invalid Action"),
    }
}

/// GENA subscriptions are accepted so renderers that require them keep working; no events
/// are sent because the content tree has no live update counter.
async fn events(State(state): State<AppState>, Peer(peer): Peer, method: Method) -> Response {
    if let Err(status) = guard(&state, peer).await {
        return status.into_response();
    }
    match method.as_str() {
        "SUBSCRIBE" => {
            let sid = format!("uuid:{}", state.services.dlna.udn().await.unwrap_or_default());
            let mut response = StatusCode::OK.into_response();
            let headers = response.headers_mut();
            if let Ok(value) = HeaderValue::from_str(&sid) {
                headers.insert("sid", value);
            }
            headers.insert("timeout", HeaderValue::from_static("Second-1800"));
            headers.insert(header::CONTENT_LENGTH, HeaderValue::from_static("0"));
            response
        }
        "UNSUBSCRIBE" => StatusCode::OK.into_response(),
        _ => StatusCode::METHOD_NOT_ALLOWED.into_response(),
    }
}

async fn media(
    State(state): State<AppState>,
    Peer(peer): Peer,
    method: Method,
    Path(file): Path<String>,
    headers: HeaderMap,
) -> Response {
    if let Err(status) = guard(&state, peer).await {
        return status.into_response();
    }
    let id = file.split_once('.').map_or(file.as_str(), |(id, _)| id);
    let Some(part_id) = id.parse::<i64>().ok().filter(|id| *id > 0).map(PartId) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let dlna = &state.services.dlna;
    let (part, mime) = match dlna.media(part_id, profile(&headers)).await {
        Ok(found) => found,
        Err(Error::NotFound | Error::Forbidden | Error::Validation(_)) => {
            return StatusCode::NOT_FOUND.into_response();
        }
        Err(error) => {
            tracing::warn!(%error, "DLNA media lookup failed");
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
    };
    let range = headers.get(header::RANGE).and_then(|v| v.to_str().ok());
    let mut response = match dlna.serve(&part, range, method == Method::HEAD).await {
        Ok(response) => response,
        Err(error) => {
            tracing::warn!(part_id = part_id.0, %error, "DLNA media unavailable");
            return StatusCode::NOT_FOUND.into_response();
        }
    };
    tracing::info!(part_id = part_id.0, status = response.status().as_u16(), "DLNA media served");
    let out = response.headers_mut();
    out.insert(header::CONTENT_TYPE, HeaderValue::from_static(mime));
    if let Ok(features) = HeaderValue::from_str(&dlna::content_features(mime)) {
        out.insert("contentfeatures.dlna.org", features);
    }
    out.insert("transfermode.dlna.org", HeaderValue::from_static(dlna::transfer_mode(mime)));
    out.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    response
}
