//! SQLite access: connection pools, migrations and repositories.
//!
//! Writes always go through [`Db::write`] (a single connection), reads through [`Db::read`].

mod pool;
pub mod repo;

pub use pool::{Db, DbOptions};

/// Map a UNIQUE constraint violation to `Conflict`, everything else through the default mapping.
pub(crate) fn map_unique(err: sqlx::Error, message: &str) -> mbl_core::Error {
    match &err {
        sqlx::Error::Database(db) if db.is_unique_violation() => {
            mbl_core::Error::Conflict(message.to_owned())
        }
        _ => err.into(),
    }
}
