//! REST API: axum routers, DTOs and error mapping. Handlers stay thin and delegate to
//! `mbl-service` once it has use cases (IMPLEMENTATION.md §1.2).

pub mod error;
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
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
