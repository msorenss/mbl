//! SQLite access: connection pools, migrations and repositories.
//!
//! Writes always go through [`Db::write`] (a single connection), reads through [`Db::read`].

mod pool;
pub mod repo;

pub use pool::{Db, DbOptions};
