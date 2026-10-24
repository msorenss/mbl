use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use mbl_core::Error;
use serde_json::json;

/// HTTP wrapper around the domain error (IMPLEMENTATION.md Appendix A.1).
#[derive(Debug)]
pub struct ApiError(pub Error);

impl From<Error> for ApiError {
    fn from(err: Error) -> Self {
        Self(err)
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(err: sqlx::Error) -> Self {
        Self(err.into())
    }
}

impl ApiError {
    fn status(&self) -> StatusCode {
        match &self.0 {
            Error::NotFound => StatusCode::NOT_FOUND,
            Error::Unauthorized => StatusCode::UNAUTHORIZED,
            Error::Forbidden => StatusCode::FORBIDDEN,
            Error::Conflict(_) => StatusCode::CONFLICT,
            Error::Validation(_) => StatusCode::UNPROCESSABLE_ENTITY,
            Error::External(_) | Error::Unavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
            Error::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status();
        let message = match &self.0 {
            Error::Internal(source) => {
                tracing::error!(error = %source, "internal error");
                "internal server error".to_owned()
            }
            other => other.to_string(),
        };
        let body = json!({ "error": { "code": self.0.code(), "message": message } });
        (status, Json(body)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_status_codes() {
        assert_eq!(ApiError(Error::NotFound).status(), StatusCode::NOT_FOUND);
        assert_eq!(
            ApiError(Error::Validation("x".into())).status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            ApiError(Error::internal(std::io::Error::other("secret"))).status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }
}
