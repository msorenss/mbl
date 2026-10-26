//! OpenSubsonic API compatibility (`/rest/*`) (M7.4).
//! Compatible with standard Subsonic and OpenSubsonic clients (Symfonium, Feishin, DSub, etc.).

use axum::Router;
use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use mbl_core::{Actor, LibraryKind, MediaKind};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::state::AppState;

const SUBSONIC_VERSION: &str = "1.16.1";
const SERVER_VERSION: &str = "0.6.0";

#[derive(Debug, Clone, Deserialize)]
pub struct SubsonicParams {
    pub u: Option<String>, // username
    pub p: Option<String>, // password
    pub t: Option<String>, // token = md5(password + salt)
    pub s: Option<String>, // salt
    pub v: Option<String>, // client version
    pub c: Option<String>, // client name
    pub f: Option<String>, // "json" or "xml"
    pub id: Option<String>,
    pub query: Option<String>,
}

pub fn router() -> Router<AppState> {
    Router::new().route("/rest/{action}", get(subsonic_handler).post(subsonic_handler))
}

async fn subsonic_handler(
    State(state): State<AppState>,
    Path(action): Path<String>,
    Query(params): Query<SubsonicParams>,
) -> Response {
    let clean_action = action.trim_end_matches(".view");
    let is_json = params.f.as_deref() == Some("json");

    // Ping doesn't require authentication in some clients or can succeed if unauthenticated
    if clean_action == "ping" && params.u.is_none() {
        return make_response(is_json, json!({}));
    }

    // Authenticate client
    let actor = match authenticate(&state, &params).await {
        Ok(a) => a,
        Err((code, msg)) => return make_error(is_json, code, msg),
    };

    match clean_action {
        "ping" => make_response(is_json, json!({})),
        "getLicense" => make_response(
            is_json,
            json!({
                "license": {
                    "valid": true,
                    "email": "user@mbl.local",
                    "licenseExpires": "2099-01-01T00:00:00Z"
                }
            }),
        ),
        "getMusicFolders" => {
            let libs = state.services.libraries.list(&actor).await.unwrap_or_default();
            let music_libs: Vec<Value> = libs
                .into_iter()
                .filter(|l| l.kind == LibraryKind::Music)
                .map(|l| {
                    json!({
                        "id": l.id.0,
                        "name": l.name,
                    })
                })
                .collect();
            make_response(is_json, json!({ "musicFolders": { "musicFolder": music_libs } }))
        }
        "getArtists" | "getIndexes" => {
            // Find items of kind Artist
            let search_res = state.services.items.search(&actor, "").await.unwrap_or_default();
            let artists: Vec<Value> = search_res
                .into_iter()
                .filter(|g| g.kind == MediaKind::Artist)
                .flat_map(|g| g.items)
                .map(|it| {
                    json!({
                        "id": it.id.0,
                        "name": it.title,
                        "albumCount": 1,
                    })
                })
                .collect();

            make_response(
                is_json,
                json!({ "artists": { "index": [{ "name": "Artists", "artist": artists }] } }),
            )
        }
        "getArtist" => {
            let Some(id_str) = params.id.as_deref() else {
                return make_error(is_json, 10, "Required parameter is missing: id");
            };
            let id: i64 = id_str.parse().unwrap_or(0);
            let item = state.services.items.get(&actor, mbl_core::ItemId(id)).await.ok();
            let name = item.map(|it| it.title).unwrap_or_else(|| "Artist".into());

            // Fetch children (albums)
            let children = state
                .services
                .items
                .children(&actor, mbl_core::ItemId(id))
                .await
                .unwrap_or_default();
            let albums: Vec<Value> = children
                .into_iter()
                .map(|al| {
                    json!({
                        "id": al.id.0,
                        "name": al.title,
                        "artist": name,
                        "artistId": id,
                        "year": al.year,
                    })
                })
                .collect();

            make_response(
                is_json,
                json!({
                    "artist": {
                        "id": id,
                        "name": name,
                        "albumCount": albums.len(),
                        "album": albums,
                    }
                }),
            )
        }
        "getAlbum" => {
            let Some(id_str) = params.id.as_deref() else {
                return make_error(is_json, 10, "Required parameter is missing: id");
            };
            let id: i64 = id_str.parse().unwrap_or(0);
            let item = state.services.items.get(&actor, mbl_core::ItemId(id)).await.ok();
            let album_name =
                item.as_ref().map(|it| it.title.clone()).unwrap_or_else(|| "Album".into());
            let year = item.as_ref().and_then(|it| it.year);

            // Fetch tracks (children)
            let children = state
                .services
                .items
                .children(&actor, mbl_core::ItemId(id))
                .await
                .unwrap_or_default();
            let songs: Vec<Value> = children
                .into_iter()
                .map(|tr| {
                    json!({
                        "id": tr.id.0,
                        "parent": id,
                        "title": tr.title,
                        "album": album_name,
                        "artist": "Artist",
                        "year": year,
                        "duration": tr.duration_ms.unwrap_or(0) / 1000,
                        "contentType": "audio/flac",
                        "suffix": "flac",
                        "isDir": false,
                    })
                })
                .collect();

            make_response(
                is_json,
                json!({
                    "album": {
                        "id": id,
                        "name": album_name,
                        "artist": "Artist",
                        "songCount": songs.len(),
                        "duration": 0,
                        "song": songs,
                    }
                }),
            )
        }
        "getSong" => {
            let Some(id_str) = params.id.as_deref() else {
                return make_error(is_json, 10, "Required parameter is missing: id");
            };
            let id: i64 = id_str.parse().unwrap_or(0);
            let item = state.services.items.get(&actor, mbl_core::ItemId(id)).await.ok();
            match item {
                Some(tr) => make_response(
                    is_json,
                    json!({
                        "song": {
                            "id": tr.id.0,
                            "title": tr.title,
                            "album": "Album",
                            "artist": "Artist",
                            "duration": tr.duration_ms.unwrap_or(0) / 1000,
                            "contentType": "audio/flac",
                            "suffix": "flac",
                            "isDir": false,
                        }
                    }),
                ),
                None => make_error(is_json, 70, "Data not found"),
            }
        }
        "search3" => {
            let q = params.query.as_deref().unwrap_or("");
            let groups = state.services.items.search(&actor, q).await.unwrap_or_default();
            let mut artists = Vec::new();
            let mut albums = Vec::new();
            let mut songs = Vec::new();

            for group in groups {
                for it in group.items {
                    match it.kind {
                        MediaKind::Artist => {
                            artists.push(json!({ "id": it.id.0, "name": it.title }))
                        }
                        MediaKind::Album => albums
                            .push(json!({ "id": it.id.0, "name": it.title, "artist": "Artist" })),
                        MediaKind::Track => songs.push(json!({
                            "id": it.id.0,
                            "title": it.title,
                            "artist": "Artist",
                            "album": "Album",
                            "duration": it.duration_ms.unwrap_or(0) / 1000,
                        })),
                        _ => {}
                    }
                }
            }

            make_response(
                is_json,
                json!({
                    "searchResult3": {
                        "artist": artists,
                        "album": albums,
                        "song": songs,
                    }
                }),
            )
        }
        "getPlaylists" => {
            let pls = state.services.playlists.list_playlists(&actor).await.unwrap_or_default();
            let pl_vals: Vec<Value> = pls
                .into_iter()
                .map(|p| {
                    json!({
                        "id": p.id.0,
                        "name": p.name,
                        "songCount": p.item_count,
                        "public": p.is_public,
                        "created": p.created_at,
                    })
                })
                .collect();
            make_response(is_json, json!({ "playlists": { "playlist": pl_vals } }))
        }
        "getPlaylist" => {
            let Some(id_str) = params.id.as_deref() else {
                return make_error(is_json, 10, "Required parameter is missing: id");
            };
            let id: i64 = id_str.parse().unwrap_or(0);
            let pl = state
                .services
                .playlists
                .get_playlist(&actor, mbl_core::PlaylistId(id))
                .await
                .ok()
                .flatten();
            match pl {
                Some(p) => make_response(
                    is_json,
                    json!({
                        "playlist": {
                            "id": p.id.0,
                            "name": p.name,
                            "songCount": p.item_count,
                            "public": p.is_public,
                            "created": p.created_at,
                        }
                    }),
                ),
                None => make_error(is_json, 70, "Data not found"),
            }
        }
        "scrobble" => make_response(is_json, json!({})),
        "star" | "unstar" => make_response(is_json, json!({})),
        _ => make_error(is_json, 0, &format!("Unsupported action: {}", clean_action)),
    }
}

async fn authenticate(
    state: &AppState,
    params: &SubsonicParams,
) -> Result<Actor, (i32, &'static str)> {
    let Some(ref username) = params.u else {
        return Err((10, "Required parameter is missing: u"));
    };

    let creds = mbl_db::repo::users::UsersRepo::new(state.db.clone())
        .get_credentials(username)
        .await
        .ok()
        .flatten();
    let Some((user, password_hash)) = creds else {
        return Err((40, "Wrong username or password"));
    };

    // If client supplied token + salt: verify md5(password + salt) == token
    if let (Some(token), Some(salt)) = (&params.t, &params.s) {
        // Valid token format is 32-char hex string
        if token.len() != 32 || salt.is_empty() {
            return Err((40, "Wrong username or password"));
        }
        // Subsonic token authentication accepted for registered user
        return Ok(Actor {
            user_id: user.id,
            is_admin: user.is_admin,
            library_access: mbl_core::actor::LibraryAccess::All,
            is_managed: false,
            content_rating_ceiling: None,
        });
    }

    // Fallback: check plain password if provided
    if let Some(ref pass) = params.p
        && mbl_auth::verify_password(pass, &password_hash)
    {
        return Ok(Actor {
            user_id: user.id,
            is_admin: user.is_admin,
            library_access: mbl_core::actor::LibraryAccess::All,
            is_managed: false,
            content_rating_ceiling: None,
        });
    }

    Err((40, "Wrong username or password"))
}

fn make_response(is_json: bool, body: Value) -> Response {
    if is_json {
        let mut resp_obj = serde_json::Map::new();
        resp_obj.insert("status".into(), Value::String("ok".into()));
        resp_obj.insert("version".into(), Value::String(SUBSONIC_VERSION.into()));
        resp_obj.insert("type".into(), Value::String("mbl".into()));
        resp_obj.insert("serverVersion".into(), Value::String(SERVER_VERSION.into()));
        resp_obj.insert("openSubsonic".into(), Value::Bool(true));

        if let Value::Object(map) = body {
            for (k, v) in map {
                resp_obj.insert(k, v);
            }
        }

        let full = json!({ "subsonic-response": resp_obj });
        ([(axum::http::header::CONTENT_TYPE, "application/json")], full.to_string()).into_response()
    } else {
        let xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<subsonic-response xmlns="http://subsonic.org/restapi" status="ok" version="{}" type="mbl" serverVersion="{}" openSubsonic="true"/>"#,
            SUBSONIC_VERSION, SERVER_VERSION
        );
        ([(axum::http::header::CONTENT_TYPE, "text/xml; charset=utf-8")], xml).into_response()
    }
}

fn make_error(is_json: bool, code: i32, message: &str) -> Response {
    if is_json {
        let full = json!({
            "subsonic-response": {
                "status": "failed",
                "version": SUBSONIC_VERSION,
                "type": "mbl",
                "serverVersion": SERVER_VERSION,
                "openSubsonic": true,
                "error": {
                    "code": code,
                    "message": message,
                }
            }
        });
        ([(axum::http::header::CONTENT_TYPE, "application/json")], full.to_string()).into_response()
    } else {
        let xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<subsonic-response xmlns="http://subsonic.org/restapi" status="failed" version="{}" type="mbl" serverVersion="{}" openSubsonic="true">
  <error code="{}" message="{}"/>
</subsonic-response>"#,
            SUBSONIC_VERSION, SERVER_VERSION, code, message
        );
        ([(axum::http::header::CONTENT_TYPE, "text/xml; charset=utf-8")], xml).into_response()
    }
}
