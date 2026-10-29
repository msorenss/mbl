//! OS abstractions: default directories, system memory, thermal monitoring and the platform profile.

pub mod dirs;
pub mod disk;
pub mod netfs;
pub mod system;
pub mod thermal;

pub use dirs::TranscodeDirSelection;
pub use system::{PlatformDefaults, PlatformProfile};
pub use thermal::ThermalStatus;
