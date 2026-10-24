//! Core domain types shared by all MBL crates: errors, IDs, kinds, pagination and time helpers.

pub mod error;
pub mod ids;
pub mod kinds;
pub mod pagination;
pub mod time;

pub use error::{Error, Result};
pub use ids::*;
pub use kinds::{LibraryKind, MediaKind, StreamKind};
pub use pagination::{Cursor, Paginated};
