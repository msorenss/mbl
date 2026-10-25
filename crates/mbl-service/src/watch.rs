//! Watch state and watch history service (`WatchService`).

use mbl_core::events::{Event, EventBus};
use mbl_core::time::now_ms;
use mbl_core::{Actor, Error, ItemId, Result};
use mbl_db::Db;
use mbl_db::repo::watch::{self, WatchHistoryEntry};

/// 1 minute in milliseconds (`60_000`).
pub const MIN_RESUME_OFFSET_MS: u64 = 60_000;
/// 3 minutes in milliseconds (`180_000`).
pub const THREE_MINUTES_MS: u64 = 180_000;

/// Returns `true` when `position_ms` exceeds 90% of `duration_ms`, or when fewer than 3 minutes
/// remain on an item longer than 3 minutes.
pub fn is_watched_threshold(position_ms: u64, duration_ms: u64) -> bool {
    if duration_ms == 0 {
        return false;
    }
    let over_90_pct = position_ms.saturating_mul(10) >= duration_ms.saturating_mul(9);
    let under_3_min_remaining = duration_ms > THREE_MINUTES_MS
        && position_ms > MIN_RESUME_OFFSET_MS
        && duration_ms.saturating_sub(position_ms) < THREE_MINUTES_MS;
    over_90_pct || under_3_min_remaining
}

/// Returns `true` when `1 minute < position_ms < 90%` (and not already meeting the watched threshold).
pub fn is_resume_threshold(position_ms: u64, duration_ms: u64) -> bool {
    if position_ms <= MIN_RESUME_OFFSET_MS {
        return false;
    }
    if is_watched_threshold(position_ms, duration_ms) {
        return false;
    }
    duration_ms == 0 || position_ms.saturating_mul(10) < duration_ms.saturating_mul(9)
}

#[derive(Debug, Clone)]
pub struct WatchService {
    db: Db,
    events: EventBus,
}

impl WatchService {
    pub fn new(db: Db, events: EventBus) -> Self {
        Self { db, events }
    }

    /// Mark an item (and its descendants if it is a show, season, artist, or album) watched.
    pub async fn mark_watched(&self, actor: &Actor, item_id: ItemId) -> Result<()> {
        let (library_id, ids) =
            watch::cascade_item_ids(&self.db, item_id).await?.ok_or(Error::NotFound)?;
        if !actor.can_access_library(library_id) {
            return Err(Error::NotFound);
        }
        watch::set_watched_batch(&self.db, actor.user_id, &ids, now_ms()).await?;
        self.events.publish(Event::ItemUpdated { item_id });
        Ok(())
    }

    /// Mark an item (and its descendants if it is a show, season, artist, or album) unwatched.
    pub async fn mark_unwatched(&self, actor: &Actor, item_id: ItemId) -> Result<()> {
        let (library_id, ids) =
            watch::cascade_item_ids(&self.db, item_id).await?.ok_or(Error::NotFound)?;
        if !actor.can_access_library(library_id) {
            return Err(Error::NotFound);
        }
        watch::set_unwatched_batch(&self.db, actor.user_id, &ids).await?;
        self.events.publish(Event::ItemUpdated { item_id });
        Ok(())
    }

    /// Apply playback progress rules for `(actor.user_id, item_id)`:
    /// - If `is_watched_threshold`, marks watched (`view_offset_ms = 0`, increments `view_count` if `increment_count` is true) and returns `Ok(true)`.
    /// - Else if `is_resume_threshold`, stores `view_offset_ms = position_ms` and returns `Ok(false)`.
    pub async fn record_progress(
        &self,
        actor: &Actor,
        item_id: ItemId,
        position_ms: u64,
        duration_ms: u64,
        increment_count: bool,
    ) -> Result<bool> {
        let now = now_ms();
        if is_watched_threshold(position_ms, duration_ms) {
            watch::complete_playback(&self.db, actor.user_id, item_id, increment_count, now)
                .await?;
            self.events.publish(Event::ItemUpdated { item_id });
            return Ok(true);
        }
        if is_resume_threshold(position_ms, duration_ms) {
            let offset = i64::try_from(position_ms).unwrap_or(i64::MAX);
            watch::save_progress_offset(&self.db, actor.user_id, item_id, offset, now).await?;
        }
        Ok(false)
    }

    /// Return recent watch history entries for `actor.user_id`.
    pub async fn history(&self, actor: &Actor, limit: u32) -> Result<Vec<WatchHistoryEntry>> {
        watch::list_history(&self.db, actor.user_id, limit.clamp(1, 200)).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thresholds_match_spec() {
        let two_hours = 120 * 60_000;
        // <= 1 min: neither watched nor resume
        assert!(!is_watched_threshold(30_000, two_hours));
        assert!(!is_resume_threshold(30_000, two_hours));
        assert!(!is_resume_threshold(60_000, two_hours));

        // 1 min < pos < 90%: resume
        assert!(is_resume_threshold(60_001, two_hours));
        assert!(is_resume_threshold(30 * 60_000, two_hours));

        // >= 90%: watched
        assert!(is_watched_threshold(108 * 60_000, two_hours));
        assert!(!is_resume_threshold(108 * 60_000, two_hours));

        // 25-minute episode: 88% (22 min, 3 min left) -> resume; 22m01s (<3 min left) -> watched
        let ep_25m = 25 * 60_000;
        assert!(is_resume_threshold(22 * 60_000, ep_25m));
        assert!(is_watched_threshold(22 * 60_000 + 1_000, ep_25m));
    }
}
