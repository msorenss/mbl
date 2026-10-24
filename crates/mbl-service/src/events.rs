//! Per-actor filtering of bus events for push channels (WebSocket, MCP subscriptions).

use mbl_core::events::{Event, EventBus};
use mbl_core::{Actor, LibraryAccess};
use mbl_db::Db;
use tokio::sync::broadcast;

#[derive(Debug, Clone)]
pub struct EventService {
    db: Db,
    bus: EventBus,
}

impl EventService {
    pub fn new(db: Db, bus: EventBus) -> Self {
        Self { db, bus }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.bus.subscribe()
    }

    /// Whether `actor` may see `event`. Job events are admin-only; library and item events
    /// require access to the library.
    pub async fn visible(&self, actor: &Actor, event: &Event) -> bool {
        match event {
            Event::JobUpdated { .. } => actor.is_admin,
            Event::ScanProgress { library_id, .. }
            | Event::LibraryUpdated { library_id }
            | Event::LibraryUnavailable { library_id, .. } => actor.can_access_library(*library_id),
            Event::ItemUpdated { item_id } => {
                if actor.library_access == LibraryAccess::All {
                    return true;
                }
                match mbl_db::repo::media::library_of_item(&self.db, *item_id).await {
                    Ok(Some(lib)) => actor.can_access_library(lib),
                    _ => false,
                }
            }
        }
    }
}
