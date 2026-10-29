//! Server wiring: configuration, logging, startup and shutdown.

pub mod app;
pub mod config;
pub mod logging;
pub mod recorder;
pub mod shutdown;
pub mod web;

pub use app::{Server, build};
pub use config::{CliOverrides, Config};
