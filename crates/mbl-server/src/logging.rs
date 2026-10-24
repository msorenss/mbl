use camino::Utf8Path;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer, Registry, fmt};

use crate::config::LogConfig;

/// Install the global subscriber: stdout plus a daily-rotated file in `logs_dir`.
/// Keep the returned guard alive for the lifetime of the process.
pub fn init(cfg: &LogConfig, logs_dir: &Utf8Path) -> anyhow::Result<WorkerGuard> {
    std::fs::create_dir_all(logs_dir)?;
    let appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("mbl")
        .filename_suffix("log")
        .max_log_files(14)
        .build(logs_dir)?;
    let (file_writer, guard) = tracing_appender::non_blocking(appender);

    let filter = EnvFilter::try_new(&cfg.level)
        .map_err(|e| anyhow::anyhow!("invalid log.level {:?}: {e}", cfg.level))?;

    let layers: Vec<Box<dyn Layer<Registry> + Send + Sync>> = if cfg.json {
        vec![
            fmt::layer().json().boxed(),
            fmt::layer().json().with_writer(file_writer).boxed(),
        ]
    } else {
        vec![
            fmt::layer().boxed(),
            fmt::layer()
                .with_ansi(false)
                .with_writer(file_writer)
                .boxed(),
        ]
    };

    tracing_subscriber::registry()
        .with(layers)
        .with(filter)
        .try_init()?;
    Ok(guard)
}
