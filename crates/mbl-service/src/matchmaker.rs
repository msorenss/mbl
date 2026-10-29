//! Movie Night Matchmaker ("Swipe to Match") service (M7.19).

use mbl_core::{Actor, Error, ItemId, Result};
use mbl_db::Db;
use mbl_db::repo::m7;
pub use mbl_db::repo::m7::MatchmakerSessionRow;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateMatchmakerSessionRequest {
    pub filter_config: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MatchmakerVoteRequest {
    pub user_name: String,
    pub item_id: ItemId,
    pub vote: i32, // 1 for right (like), 0 for left (pass)
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MatchmakerMatchResult {
    pub matched: bool,
    pub matched_item_id: Option<ItemId>,
}

#[derive(Debug, Clone)]
pub struct MatchmakerService {
    db: Arc<Db>,
}

impl MatchmakerService {
    pub fn new(db: Arc<Db>) -> Self {
        Self { db }
    }

    pub async fn create_session(
        &self,
        actor: &Actor,
        req: CreateMatchmakerSessionRequest,
    ) -> Result<MatchmakerSessionRow> {
        let user_id = actor.user_id;
        let session_id = format!("mm_{}", mbl_auth::new_token()?.value);
        let filter_config = req.filter_config.unwrap_or_else(|| "{}".into());
        if filter_config.len() > 4096
            || serde_json::from_str::<serde_json::Value>(&filter_config).is_err()
        {
            return Err(Error::Validation("filterConfig must be JSON up to 4096 bytes".into()));
        }

        m7::create_matchmaker_session(&self.db, &session_id, user_id, &filter_config).await?;
        m7::get_matchmaker_session(&self.db, &session_id).await?.ok_or(Error::NotFound)
    }

    pub async fn get_session(
        &self,
        actor: &Actor,
        id: &str,
    ) -> Result<Option<MatchmakerSessionRow>> {
        if !m7::is_matchmaker_member(&self.db, id, actor.user_id).await? {
            return Err(Error::NotFound);
        }
        let session = m7::get_matchmaker_session(&self.db, id).await?;
        if let Some(item) = session.as_ref().and_then(|s| s.matched_item_id) {
            crate::ItemService::new((*self.db).clone()).get(actor, item).await?;
        }
        Ok(session)
    }

    pub async fn join_session(&self, actor: &Actor, id: &str) -> Result<()> {
        m7::join_matchmaker(&self.db, id, actor.user_id).await
    }

    pub async fn record_vote(
        &self,
        actor: &Actor,
        session_id: &str,
        req: MatchmakerVoteRequest,
        _required_participants: usize,
    ) -> Result<MatchmakerMatchResult> {
        let session = self.get_session(actor, session_id).await?.ok_or(Error::NotFound)?;
        if req.vote != 0 && req.vote != 1 {
            return Err(Error::Validation("vote must be 0 or 1".into()));
        }
        crate::ItemService::new((*self.db).clone()).get(actor, req.item_id).await?;
        let required_participants = m7::matchmaker_member_count(&self.db, session_id).await?.max(2);

        if session.status == "matched" {
            return Ok(MatchmakerMatchResult {
                matched: true,
                matched_item_id: session.matched_item_id,
            });
        }

        m7::record_matchmaker_vote(
            &self.db,
            session_id,
            &format!("user:{}", actor.user_id.0),
            req.item_id,
            req.vote,
        )
        .await?;

        // Check if all participants voted positive for this item
        if let Some(matched_item) =
            m7::check_matchmaker_matches(&self.db, session_id, required_participants).await?
        {
            m7::complete_matchmaker_session(&self.db, session_id, matched_item).await?;
            Ok(MatchmakerMatchResult { matched: true, matched_item_id: Some(matched_item) })
        } else {
            Ok(MatchmakerMatchResult { matched: false, matched_item_id: None })
        }
    }
}
