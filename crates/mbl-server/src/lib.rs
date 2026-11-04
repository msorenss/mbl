//! Server wiring: configuration, logging, startup and shutdown.

pub mod app;
pub mod config;
pub mod logging;
pub mod mdns;
pub mod mqtt;
pub mod recorder;
pub mod shutdown;
pub mod tls;
pub mod web;
pub mod winservice;

pub use app::{Server, build};
pub use config::{CliOverrides, Config};
