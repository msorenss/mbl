//! Collections and Playlists service layer (M7.6).

use mbl_core::{Actor, CollectionId, Error, ItemId, PlaylistId, Result};
use mbl_db::Db;
pub use mbl_db::repo::m7::{CollectionRow, PlaylistRow};
use mbl_db::repo::{items::visible_ids, m7};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateCollectionRequest {
    pub name: String,
    pub overview: Option<String>,
    pub is_smart: Option<bool>,
    pub smart_filter: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCollectionRequest {
    pub name: String,
    pub overview: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreatePlaylistRequest {
    pub name: String,
    pub is_public: Option<bool>,
    pub is_smart: Option<bool>,
    pub smart_filter: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdatePlaylistRequest {
    pub name: String,
    pub is_public: bool,
}

#[derive(Debug, Clone)]
pub struct CollectionService {
    db: Arc<Db>,
}

impl CollectionService {
    pub fn new(db: Arc<Db>) -> Self {
        Self { db }
    }

    pub async fn create_collection(
        &self,
        actor: &Actor,
        req: CreateCollectionRequest,
    ) -> Result<CollectionRow> {
        actor.require_admin()?;
        let id = m7::create_collection(
            &self.db,
            &req.name,
            req.overview.as_deref(),
            req.is_smart.unwrap_or(false),
            req.smart_filter.as_deref(),
        )
        .await?;

        m7::get_collection(&self.db, id).await?.ok_or(Error::NotFound)
    }

    pub async fn get_collection(
        &self,
        actor: &Actor,
        id: CollectionId,
    ) -> Result<Option<CollectionRow>> {
        let mut row = m7::get_collection(&self.db, id).await?;
        if let Some(ref mut collection) = row {
            collection.item_count = self.get_items(actor, id).await?.len() as i64;
        }
        Ok(row)
    }

    pub async fn list_collections(&self, actor: &Actor) -> Result<Vec<CollectionRow>> {
        let mut rows = m7::list_collections(&self.db).await?;
        for row in &mut rows {
            row.item_count = self.get_items(actor, row.id).await?.len() as i64;
        }
        Ok(rows)
    }

    pub async fn update_collection(
        &self,
        actor: &Actor,
        id: CollectionId,
        req: UpdateCollectionRequest,
    ) -> Result<()> {
        actor.require_admin()?;
        if !m7::update_collection(&self.db, id, &req.name, req.overview.as_deref()).await? {
            return Err(Error::NotFound);
        }
        Ok(())
    }

    pub async fn delete_collection(&self, actor: &Actor, id: CollectionId) -> Result<()> {
        actor.require_admin()?;
        if !m7::delete_collection(&self.db, id).await? {
            return Err(Error::NotFound);
        }
        Ok(())
    }

    pub async fn add_item(&self, actor: &Actor, id: CollectionId, item_id: ItemId) -> Result<()> {
        actor.require_admin()?;
        m7::get_collection(&self.db, id).await?.ok_or(Error::NotFound)?;
        require_visible_item(&self.db, actor, item_id).await?;
        m7::add_item_to_collection(&self.db, id, item_id).await
    }

    pub async fn remove_item(
        &self,
        actor: &Actor,
        id: CollectionId,
        item_id: ItemId,
    ) -> Result<()> {
        actor.require_admin()?;
        if !m7::remove_item_from_collection(&self.db, id, item_id).await? {
            return Err(Error::NotFound);
        }
        Ok(())
    }

    pub async fn get_items(&self, actor: &Actor, id: CollectionId) -> Result<Vec<ItemId>> {
        m7::get_collection(&self.db, id).await?.ok_or(Error::NotFound)?;
        let ids = m7::get_collection_item_ids(&self.db, id).await?;
        visible_ids(&self.db, actor, &ids).await
    }
}

#[derive(Debug, Clone)]
pub struct PlaylistService {
    db: Arc<Db>,
}

impl PlaylistService {
    pub fn new(db: Arc<Db>) -> Self {
        Self { db }
    }

    pub async fn create_playlist(
        &self,
        actor: &Actor,
        req: CreatePlaylistRequest,
    ) -> Result<PlaylistRow> {
        let user_id = actor.user_id;
        let id = m7::create_playlist(
            &self.db,
            user_id,
            &req.name,
            req.is_public.unwrap_or(false),
            req.is_smart.unwrap_or(false),
            req.smart_filter.as_deref(),
        )
        .await?;

        m7::get_playlist(&self.db, id).await?.ok_or(Error::NotFound)
    }

    pub async fn get_playlist(&self, actor: &Actor, id: PlaylistId) -> Result<Option<PlaylistRow>> {
        let mut row = m7::get_playlist(&self.db, id).await?;
        if let Some(ref pl) = row
            && !pl.is_public
            && !actor.is_admin
            && actor.user_id != pl.user_id
        {
            return Err(Error::Forbidden);
        }
        if let Some(ref mut playlist) = row {
            playlist.item_count = self.get_items(actor, id).await?.len() as i64;
        }
        Ok(row)
    }

    pub async fn list_playlists(&self, actor: &Actor) -> Result<Vec<PlaylistRow>> {
        let mut rows = m7::list_playlists_for_user(&self.db, actor.user_id).await?;
        for row in &mut rows {
            row.item_count = self.get_items(actor, row.id).await?.len() as i64;
        }
        Ok(rows)
    }

    pub async fn update_playlist(
        &self,
        actor: &Actor,
        id: PlaylistId,
        req: UpdatePlaylistRequest,
    ) -> Result<()> {
        let pl = m7::get_playlist(&self.db, id).await?.ok_or(Error::NotFound)?;
        if !actor.is_admin && actor.user_id != pl.user_id {
            return Err(Error::Forbidden);
        }

        if !m7::update_playlist(&self.db, id, &req.name, req.is_public).await? {
            return Err(Error::NotFound);
        }
        Ok(())
    }

    pub async fn delete_playlist(&self, actor: &Actor, id: PlaylistId) -> Result<()> {
        let pl = m7::get_playlist(&self.db, id).await?.ok_or(Error::NotFound)?;
        if !actor.is_admin && actor.user_id != pl.user_id {
            return Err(Error::Forbidden);
        }

        if !m7::delete_playlist(&self.db, id).await? {
            return Err(Error::NotFound);
        }
        Ok(())
    }

    pub async fn add_item(&self, actor: &Actor, id: PlaylistId, item_id: ItemId) -> Result<()> {
        let pl = m7::get_playlist(&self.db, id).await?.ok_or(Error::NotFound)?;
        if !actor.is_admin && actor.user_id != pl.user_id {
            return Err(Error::Forbidden);
        }
        require_visible_item(&self.db, actor, item_id).await?;
        m7::add_item_to_playlist(&self.db, id, item_id).await
    }

    pub async fn remove_item(&self, actor: &Actor, id: PlaylistId, item_id: ItemId) -> Result<()> {
        let pl = m7::get_playlist(&self.db, id).await?.ok_or(Error::NotFound)?;
        if !actor.is_admin && actor.user_id != pl.user_id {
            return Err(Error::Forbidden);
        }
        if !m7::remove_item_from_playlist(&self.db, id, item_id).await? {
            return Err(Error::NotFound);
        }
        Ok(())
    }

    pub async fn get_items(&self, actor: &Actor, id: PlaylistId) -> Result<Vec<ItemId>> {
        let pl = m7::get_playlist(&self.db, id).await?.ok_or(Error::NotFound)?;
        if !pl.is_public && !actor.is_admin && actor.user_id != pl.user_id {
            return Err(Error::Forbidden);
        }
        let ids = m7::get_playlist_item_ids(&self.db, id).await?;
        visible_ids(&self.db, actor, &ids).await
    }
}

async fn require_visible_item(db: &Db, actor: &Actor, id: ItemId) -> Result<()> {
    if visible_ids(db, actor, &[id]).await?.is_empty() {
        return Err(Error::NotFound);
    }
    Ok(())
}
