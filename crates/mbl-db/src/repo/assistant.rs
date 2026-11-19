//! Conversations with the built-in assistant. Every query is scoped to the owner.

use mbl_core::{Result, UserId};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::Db;

/// Conversations kept per user; the oldest is removed beyond this.
const CONVERSATIONS_PER_USER: i64 = 50;

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Conversation {
    pub id: i64,
    pub title: String,
    pub created_at: i64,
    pub updated_at: i64,
}

/// A library title an answer mentions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ItemMention {
    pub id: i64,
    pub title: String,
    /// "movie", "show", "season", "episode"…
    pub kind: String,
    #[serde(default)]
    pub year: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConversationMessage {
    pub id: i64,
    /// "user" or "assistant".
    pub role: String,
    pub content: String,
    pub created_at: i64,
    /// Titles the answer mentions, for links.
    pub items: Vec<ItemMention>,
}

#[derive(sqlx::FromRow)]
struct MessageRow {
    id: i64,
    role: String,
    content: String,
    created_at: i64,
    items: Option<String>,
}

impl From<MessageRow> for ConversationMessage {
    fn from(row: MessageRow) -> Self {
        Self {
            id: row.id,
            role: row.role,
            content: row.content,
            created_at: row.created_at,
            items: row.items.and_then(|json| serde_json::from_str(&json).ok()).unwrap_or_default(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AssistantRepo {
    db: Db,
}

impl AssistantRepo {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub async fn list(&self, owner: UserId) -> Result<Vec<Conversation>> {
        Ok(sqlx::query_as(
            "SELECT id, title, created_at, updated_at FROM assistant_conversations
             WHERE user_id = ? ORDER BY updated_at DESC, id DESC",
        )
        .bind(owner)
        .fetch_all(&self.db.read)
        .await?)
    }

    pub async fn get(&self, owner: UserId, id: i64) -> Result<Option<Conversation>> {
        Ok(sqlx::query_as(
            "SELECT id, title, created_at, updated_at FROM assistant_conversations
             WHERE user_id = ? AND id = ?",
        )
        .bind(owner)
        .bind(id)
        .fetch_optional(&self.db.read)
        .await?)
    }

    /// A new conversation; the owner's oldest ones beyond the limit are removed.
    pub async fn create(&self, owner: UserId, title: &str, now: i64) -> Result<Conversation> {
        let mut tx = self.db.write.begin().await?;
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO assistant_conversations (user_id, title, created_at, updated_at)
             VALUES (?, ?, ?, ?) RETURNING id",
        )
        .bind(owner)
        .bind(title)
        .bind(now)
        .bind(now)
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query(
            "DELETE FROM assistant_conversations WHERE user_id = ? AND id NOT IN (
                SELECT id FROM assistant_conversations WHERE user_id = ?
                ORDER BY updated_at DESC, id DESC LIMIT ?)",
        )
        .bind(owner)
        .bind(owner)
        .bind(CONVERSATIONS_PER_USER)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(Conversation { id, title: title.to_owned(), created_at: now, updated_at: now })
    }

    /// The latest `limit` messages, oldest first.
    pub async fn messages(
        &self,
        owner: UserId,
        conversation: i64,
        limit: i64,
    ) -> Result<Vec<ConversationMessage>> {
        let mut rows: Vec<MessageRow> = sqlx::query_as(
            "SELECT m.id, m.role, m.content, m.created_at, m.items FROM assistant_messages m
             JOIN assistant_conversations c ON c.id = m.conversation_id
             WHERE c.user_id = ? AND c.id = ? ORDER BY m.id DESC LIMIT ?",
        )
        .bind(owner)
        .bind(conversation)
        .bind(limit)
        .fetch_all(&self.db.read)
        .await?;
        rows.reverse();
        Ok(rows.into_iter().map(Into::into).collect())
    }

    /// Adds a message; `None` when the conversation is not the owner's.
    pub async fn add_message(
        &self,
        owner: UserId,
        conversation: i64,
        role: &str,
        content: &str,
        items: &[ItemMention],
        now: i64,
    ) -> Result<Option<ConversationMessage>> {
        let items_json = if items.is_empty() {
            None
        } else {
            Some(serde_json::to_string(items).map_err(mbl_core::Error::internal)?)
        };
        let mut tx = self.db.write.begin().await?;
        let touched = sqlx::query(
            "UPDATE assistant_conversations SET updated_at = ? WHERE user_id = ? AND id = ?",
        )
        .bind(now)
        .bind(owner)
        .bind(conversation)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if touched == 0 {
            return Ok(None);
        }
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO assistant_messages (conversation_id, role, content, created_at, items)
             VALUES (?, ?, ?, ?, ?) RETURNING id",
        )
        .bind(conversation)
        .bind(role)
        .bind(content)
        .bind(now)
        .bind(items_json)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(Some(ConversationMessage {
            id,
            role: role.to_owned(),
            content: content.to_owned(),
            created_at: now,
            items: items.to_vec(),
        }))
    }

    pub async fn delete(&self, owner: UserId, id: i64) -> Result<bool> {
        Ok(sqlx::query("DELETE FROM assistant_conversations WHERE user_id = ? AND id = ?")
            .bind(owner)
            .bind(id)
            .execute(&self.db.write)
            .await?
            .rows_affected()
            > 0)
    }
}
