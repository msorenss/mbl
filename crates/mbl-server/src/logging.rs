use std::collections::BTreeMap;
use std::fmt::Debug;

use camino::Utf8Path;
use mbl_api::logs::{RecentLogs, ServerLogEntry};
use tracing::field::{Field, Visit};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::layer::{Context, SubscriberExt};
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer, Registry, fmt};

use crate::config::LogConfig;

/// Install the global subscriber: stdout plus a daily-rotated file in `logs_dir`.
/// Keep the returned guard alive for the lifetime of the process.
pub fn init(cfg: &LogConfig, logs_dir: &Utf8Path) -> anyhow::Result<(WorkerGuard, RecentLogs)> {
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
        vec![fmt::layer().json().boxed(), fmt::layer().json().with_writer(file_writer).boxed()]
    } else {
        vec![fmt::layer().boxed(), fmt::layer().with_ansi(false).with_writer(file_writer).boxed()]
    };

    let recent = RecentLogs::default();
    tracing_subscriber::registry()
        .with(layers)
        .with(WebLogLayer(recent.clone()))
        .with(filter)
        .try_init()?;
    Ok((guard, recent))
}

struct WebLogLayer(RecentLogs);

impl<S: tracing::Subscriber> Layer<S> for WebLogLayer {
    fn on_event(&self, event: &tracing::Event<'_>, _: Context<'_, S>) {
        let metadata = event.metadata();
        if !metadata.target().starts_with("mbl_") {
            return;
        }

        let mut fields = WebLogFields::default();
        event.record(&mut fields);
        if let Some(message) = fields.message {
            self.0.record(ServerLogEntry {
                timestamp_ms: mbl_core::time::now_ms(),
                level: metadata.level().as_str().to_ascii_lowercase(),
                target: metadata.target().to_owned(),
                message,
                details: fields.details,
            });
        }
    }
}

#[derive(Default)]
struct WebLogFields {
    message: Option<String>,
    details: BTreeMap<String, String>,
}

impl WebLogFields {
    fn number(&mut self, field: &Field, value: impl ToString) {
        if matches!(
            field.name(),
            "item_id"
                | "part_id"
                | "library_id"
                | "start_ms"
                | "status"
                | "elapsed_ms"
                | "attempt"
                | "segment_number"
                | "variant_count"
                | "variant_index"
        ) {
            self.details.insert(field.name().to_owned(), value.to_string());
        }
    }
}

impl Visit for WebLogFields {
    fn record_debug(&mut self, field: &Field, value: &dyn Debug) {
        // First-party event messages are short descriptions. Untrusted input goes in separate
        // fields, which are excluded unless explicitly allowed here.
        if field.name() == "message" {
            let message = format!("{value:?}");
            self.record_str(field, &message);
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message = Some(value.chars().filter(|c| !c.is_control()).take(512).collect());
        } else if field.name() == "stream_asset" && matches!(value, "playlist" | "init" | "segment")
        {
            self.details.insert(field.name().to_owned(), value.to_owned());
        }
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.number(field, value);
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.number(field, value);
    }
}
