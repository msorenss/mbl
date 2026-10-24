//! OS abstractions: default directories, system memory and the platform profile.

pub mod dirs;
pub mod netfs;
pub mod system;

pub use system::PlatformProfile;
