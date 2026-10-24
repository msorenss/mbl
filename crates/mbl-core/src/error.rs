pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Domain error. Adapters (REST, MCP) map variants to transport-specific errors.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("not found")]
    NotFound,
    #[error("unauthorized")]
    Unauthorized,
    #[error("forbidden")]
    Forbidden,
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("validation: {0}")]
    Validation(String),
    #[error("external service: {0}")]
    External(String),
    #[error("unavailable: {0}")]
    Unavailable(String),
    #[error("too many requests, retry in {retry_after_secs}s")]
    RateLimited { retry_after_secs: u64 },
    #[error(transparent)]
    Internal(#[from] Box<dyn std::error::Error + Send + Sync>),
}

impl Error {
    pub fn internal<E>(err: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        Self::Internal(Box::new(err))
    }

    /// Stable machine-readable code used in API error bodies.
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotFound => "not_found",
            Self::Unauthorized => "unauthorized",
            Self::Forbidden => "forbidden",
            Self::Conflict(_) => "conflict",
            Self::Validation(_) => "validation",
            Self::External(_) => "external",
            Self::Unavailable(_) => "unavailable",
            Self::RateLimited { .. } => "rate_limited",
            Self::Internal(_) => "internal",
        }
    }
}

impl From<sqlx::Error> for Error {
    fn from(err: sqlx::Error) -> Self {
        match err {
            sqlx::Error::RowNotFound => Self::NotFound,
            other => Self::internal(other),
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Self::internal(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_not_found_maps_to_not_found() {
        let err: Error = sqlx::Error::RowNotFound.into();
        assert!(matches!(err, Error::NotFound));
        assert_eq!(err.code(), "not_found");
    }

    #[test]
    fn io_error_is_internal() {
        let err: Error = std::io::Error::other("boom").into();
        assert_eq!(err.code(), "internal");
        assert_eq!(err.to_string(), "boom");
    }
}
