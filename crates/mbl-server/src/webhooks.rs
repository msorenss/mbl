//! Turns bus events into webhook deliveries (`playback.*`, `media.added`, `library.*`).

use mbl_core::events::Event;
use mbl_core::time::now_ms;
use mbl_core::{ItemId, LibraryId, UserId};
use mbl_db::Db;
use mbl_service::Services;
use serde_json::{Value, json};
use tokio::sync::broadcast::error::RecvError;
use tokio_util::sync::CancellationToken;

pub async fn run(db: Db, services: Services, stop: CancellationToken) {
    let mut rx = services.events.subscribe();
    loop {
        let event = tokio::select! {
            () = stop.cancelled() => return,
            event = rx.recv() => event,
        };
        match event {
            Ok(event) => {
                for (name, payload) in payloads(&db, &event).await {
                    services.webhooks.dispatch_event(name, &payload).await;
                }
            }
            Err(RecvError::Lagged(missed)) => {
                tracing::warn!(missed, "webhook dispatcher fell behind; events were skipped");
            }
            Err(RecvError::Closed) => return,
        }
    }
}

/// Webhook event names and JSON bodies for one bus event (none for internal events).
pub async fn payloads(db: &Db, event: &Event) -> Vec<(&'static str, Value)> {
    let now = now_ms();
    match event {
        Event::ScanCompleted { library_id, added, missing } => {
            let library = library_name(db, *library_id).await;
            let mut out = vec![(
                "library.scan_completed",
                json!({
                    "event": "library.scan_completed", "timestamp": now,
                    "libraryId": library_id.0, "library": library,
                    "added": added, "missing": missing,
                }),
            )];
            if *added > 0 {
                out.push((
                    "media.added",
                    json!({
                        "event": "media.added", "timestamp": now,
                        "libraryId": library_id.0, "library": library, "count": added,
                    }),
                ));
            }
            out
        }
        Event::LibraryUnavailable { library_id, path } => vec![(
            "library.unavailable",
            json!({
                "event": "library.unavailable", "timestamp": now,
                "libraryId": library_id.0, "library": library_name(db, *library_id).await,
                "path": path,
            }),
        )],
        Event::PlaybackStarted { user_id, item_id, session_id } => vec![(
            "playback.started",
            json!({
                "event": "playback.started", "timestamp": now, "sessionId": session_id,
                "user": user_json(db, *user_id).await, "item": item_json(db, *item_id).await,
            }),
        )],
        Event::PlaybackStopped {
            user_id,
            item_id,
            session_id,
            position_ms,
            duration_ms,
            completed,
        } => vec![(
            "playback.stopped",
            json!({
                "event": "playback.stopped", "timestamp": now, "sessionId": session_id,
                "user": user_json(db, *user_id).await, "item": item_json(db, *item_id).await,
                "positionMs": position_ms, "durationMs": duration_ms, "completed": completed,
            }),
        )],
        Event::JobUpdated { .. }
        | Event::ScanProgress { .. }
        | Event::LibraryUpdated { .. }
        | Event::ItemUpdated { .. } => Vec::new(),
    }
}

async fn library_name(db: &Db, id: LibraryId) -> Option<String> {
    sqlx::query_scalar("SELECT name FROM libraries WHERE id = ?")
        .bind(id.0)
        .fetch_optional(&db.read)
        .await
        .ok()
        .flatten()
}

async fn user_json(db: &Db, id: UserId) -> Value {
    let name: Option<String> = sqlx::query_scalar("SELECT username FROM users WHERE id = ?")
        .bind(id.0)
        .fetch_optional(&db.read)
        .await
        .ok()
        .flatten();
    json!({ "id": id.0, "username": name })
}

/// title, kind, year, episode number, show title, season number.
type ItemRow = (String, String, Option<i64>, Option<i64>, Option<String>, Option<i64>);

/// Title, kind and year, plus show/season/episode numbers for episodes.
async fn item_json(db: &Db, id: ItemId) -> Value {
    let row: Option<ItemRow> = sqlx::query_as(
        "SELECT i.title, i.kind, i.year, i.idx, show.title, season.idx
             FROM media_items i
             LEFT JOIN media_items season ON season.id = i.parent_id AND season.kind = 'season'
             LEFT JOIN media_items show ON show.id = season.parent_id
             WHERE i.id = ?",
    )
    .bind(id.0)
    .fetch_optional(&db.read)
    .await
    .ok()
    .flatten();
    match row {
        Some((title, kind, year, episode, show, season)) => json!({
            "id": id.0, "title": title, "kind": kind, "year": year,
            "show": show, "season": season, "episode": episode,
        }),
        None => json!({ "id": id.0 }),
    }
}
