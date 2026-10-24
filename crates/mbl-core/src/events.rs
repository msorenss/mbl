//! In-process event bus. Subscribers (WebSocket, MCP resource subscriptions) filter per actor.

use serde::Serialize;
use tokio::sync::broadcast;

use crate::{ItemId, JobId, LibraryId};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", content = "data", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Event {
    JobUpdated { job_id: JobId, kind: String, status: String, progress: f32 },
    ScanProgress { library_id: LibraryId, scanned: u64, total: Option<u64>, done: bool },
    LibraryUpdated { library_id: LibraryId },
    LibraryUnavailable { library_id: LibraryId, path: String },
    ItemUpdated { item_id: ItemId },
}

/// Cheap-to-clone broadcast bus. Slow subscribers miss events rather than block publishers.
#[derive(Debug, Clone)]
pub struct EventBus {
    tx: broadcast::Sender<Event>,
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new(1024)
    }
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        Self { tx: broadcast::channel(capacity).0 }
    }

    pub fn publish(&self, event: Event) {
        // No subscribers is fine.
        let _ = self.tx.send(event);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.tx.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_tagged() {
        let e = Event::JobUpdated {
            job_id: JobId(3),
            kind: "library.scan".into(),
            status: "running".into(),
            progress: 0.5,
        };
        assert_eq!(
            serde_json::to_value(&e).unwrap(),
            serde_json::json!({ "type": "jobUpdated", "data": { "jobId": 3, "kind": "library.scan", "status": "running", "progress": 0.5 } })
        );
    }

    #[test]
    fn publish_without_subscribers_is_ok() {
        let bus = EventBus::default();
        bus.publish(Event::LibraryUpdated { library_id: LibraryId(1) });
        let mut rx = bus.subscribe();
        bus.publish(Event::LibraryUpdated { library_id: LibraryId(2) });
        assert_eq!(rx.try_recv().unwrap(), Event::LibraryUpdated { library_id: LibraryId(2) });
    }
}
