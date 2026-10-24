//! OpenAPI document and the Scalar reference UI.

use axum::Router;
use axum::http::header;
use axum::response::Html;
use axum::routing::get;
use utoipa::openapi::security::{ApiKey, ApiKeyValue, HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::{Modify, OpenApi};

use crate::AppState;
use crate::routes::{auth, events, images, items, libraries, system, users};

#[derive(OpenApi)]
#[openapi(
    info(title = "MBL API", description = "MBL media server REST API."),
    paths(
        system::health, system::info, system::setup_status, system::setup,
        auth::login, auth::logout, auth::me, auth::list_sessions, auth::revoke_session,
        users::list, users::create, users::get_one, users::update, users::remove,
        libraries::list, libraries::get_one, libraries::create, libraries::update,
        libraries::remove, libraries::scan, libraries::browse,
        items::list, items::detail, items::children, items::search, items::home,
        images::image, events::events,
    ),
    modifiers(&Security),
    security(("bearer" = []), ("cookie" = [])),
)]
pub struct ApiDoc;

struct Security;

impl Modify for Security {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            "bearer",
            SecurityScheme::Http(HttpBuilder::new().scheme(HttpAuthScheme::Bearer).build()),
        );
        components.add_security_scheme(
            "cookie",
            SecurityScheme::ApiKey(ApiKey::Cookie(ApiKeyValue::new(
                crate::extract::SESSION_COOKIE,
            ))),
        );
    }
}

/// The spec as pretty JSON with a trailing newline (the committed `docs/openapi.json` format).
pub fn spec_json() -> String {
    let mut doc = ApiDoc::openapi();
    doc.info.version = env!("CARGO_PKG_VERSION").to_owned();
    let mut json = doc.to_pretty_json().unwrap_or_default();
    json.push('\n');
    json
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/openapi.json",
            get(|| async { ([(header::CONTENT_TYPE, "application/json")], spec_json()) }),
        )
        .route("/api/docs", get(|| async { Html(SCALAR_HTML) }))
}

const SCALAR_HTML: &str = r#"<!doctype html>
<html>
<head>
  <title>MBL API</title>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
</head>
<body>
  <script id="api-reference" data-url="/api/openapi.json"></script>
  <script src="https://cdn.jsdelivr.net/npm/@scalar/api-reference"></script>
</body>
</html>
"#;
