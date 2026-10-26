//! Collections and Playlists service layer (M7.6).

use mbl_core::{Actor, CollectionId, Error, ItemId, PlaylistId, Result};
use mbl_db::Db;
use mbl_db::repo::m7;
pub use mbl_db::repo::m7::{CollectionRow, PlaylistRow};
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
        _actor: &Actor,
        id: CollectionId,
    ) -> Result<Option<CollectionRow>> {
        m7::get_collection(&self.db, id).await
    }

    pub async fn list_collections(&self, _actor: &Actor) -> Result<Vec<CollectionRow>> {
        m7::list_collections(&self.db).await
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

    pub async fn get_items(&self, _actor: &Actor, id: CollectionId) -> Result<Vec<ItemId>> {
        m7::get_collection_item_ids(&self.db, id).await
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
        let row = m7::get_playlist(&self.db, id).await?;
        if let Some(ref pl) = row
            && !pl.is_public
            && !actor.is_admin
            && actor.user_id != pl.user_id
        {
            return Err(Error::Forbidden);
        }
        Ok(row)
    }

    pub async fn list_playlists(&self, actor: &Actor) -> Result<Vec<PlaylistRow>> {
        m7::list_playlists_for_user(&self.db, actor.user_id).await
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
        m7::get_playlist_item_ids(&self.db, id).await
    }
}
