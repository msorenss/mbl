use std::path::PathBuf;

use async_trait::async_trait;
use mbl_core::{Actor, ArtworkId, Error, ItemId, Result};
use mbl_db::Db;
use mbl_db::repo::artwork::{self, Artwork};
use mbl_images::ImageStore;
use mbl_jobs::{JobCtx, JobHandler, kinds};

#[derive(Debug, Clone)]
pub struct ImageFile {
    pub path: PathBuf,
    pub hash: String,
    pub width: u32,
    pub quality: u8,
    pub blurred: bool,
}

#[derive(Debug, Clone)]
pub struct ImageService {
    db: Db,
    store: ImageStore,
}

impl ImageService {
    pub fn new(db: Db, store: ImageStore) -> Self {
        Self { db, store }
    }

    /// A resized JPEG of an artwork, downloading the original on first use.
    pub async fn get(
        &self,
        actor: &Actor,
        id: ArtworkId,
        width: u32,
        quality: u8,
    ) -> Result<ImageFile> {
        self.get_with_blur(actor, id, width, quality, false).await
    }

    /// Resized JPEG, optionally with server-side Gaussian blur for TV/ambient mode.
    pub async fn get_with_blur(
        &self,
        actor: &Actor,
        id: ArtworkId,
        width: u32,
        quality: u8,
        blur: bool,
    ) -> Result<ImageFile> {
        let art = artwork::get(&self.db, id).await?.ok_or(Error::NotFound)?;
        if !actor.can_access_library(art.library_id) {
            return Err(Error::NotFound);
        }
        let hash = match &art.local_hash {
            Some(h) => h.clone(),
            None => self.ingest(&art).await?,
        };
        let width = mbl_images::snap_width(width);
        let quality = quality.clamp(30, 95);
        let path = if blur {
            self.store.blurred_variant(&hash, width, quality).await?
        } else {
            self.store.variant(&hash, width, quality).await?
        };
        Ok(ImageFile { path, hash, width, quality, blurred: blur })
    }

    async fn ingest(&self, art: &Artwork) -> Result<String> {
        let url = art.source_url.as_deref().ok_or(Error::NotFound)?;
        let ing = self.store.ingest(url).await?;
        let palette = serde_json::to_string(&ing.palette).map_err(Error::internal)?;
        artwork::set_local(
            &self.db,
            art.id,
            &ing.hash,
            i64::from(ing.width),
            i64::from(ing.height),
            &ing.blurhash,
            &palette,
        )
        .await?;
        Ok(ing.hash)
    }

    /// Download all pending artwork of an item and its descendants. Returns how many succeeded.
    pub async fn fetch_pending(&self, root: ItemId) -> Result<usize> {
        let mut ok = 0;
        for id in artwork::pending_for_tree(&self.db, root).await? {
            let Some(art) = artwork::get(&self.db, id).await? else { continue };
            match self.ingest(&art).await {
                Ok(_) => ok += 1,
                Err(e) => {
                    tracing::warn!(artwork_id = %id, url = ?art.source_url, error = %e, "artwork fetch failed")
                }
            }
        }
        Ok(ok)
    }
}

/// `artwork.fetch` job: payload `{"itemId": n}`.
pub struct ArtworkFetchHandler {
    pub images: ImageService,
}

#[async_trait]
impl JobHandler for ArtworkFetchHandler {
    fn kind(&self) -> &'static str {
        kinds::ARTWORK_FETCH
    }

    fn concurrency(&self, _: mbl_platform::PlatformProfile) -> usize {
        2
    }

    async fn run(&self, _ctx: JobCtx, payload: serde_json::Value) -> Result<()> {
        let id = payload["itemId"]
            .as_i64()
            .map(ItemId)
            .ok_or_else(|| Error::Validation("artwork.fetch without itemId".into()))?;
        self.images.fetch_pending(id).await.map(|_| ())
    }
}
