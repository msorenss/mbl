//! REST API: axum routers, DTOs and error mapping. Handlers stay thin and delegate to
//! `mbl-service` once it has use cases (IMPLEMENTATION.md §1.2).

pub mod dto;
pub mod error;
pub mod extract;
pub mod openapi;
pub mod rate_limit;
pub mod routes;
mod state;

use axum::Router;
use tower_http::trace::TraceLayer;

pub use error::ApiError;
pub use state::{AppState, ServerInfo};

/// Build the full HTTP router.
pub fn router(state: AppState) -> Router {
    Router::new()
        .merge(routes::system::router())
        .merge(routes::auth::router())
        .merge(routes::users::router())
        .merge(routes::libraries::router())
        .merge(routes::images::router())
        .merge(routes::events::router())
        .merge(routes::items::router())
        .merge(routes::playback::router())
        .merge(routes::stream::router())
        .merge(routes::mcp::router())
        .merge(routes::hw::router())
        .merge(openapi::router())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
