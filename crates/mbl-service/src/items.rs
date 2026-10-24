//! Browsing use cases. All reads go through [`ItemFilter::for_user`].

use mbl_core::{Actor, Error, ItemId, LibraryId, MediaKind, Paginated, Result};
use mbl_db::Db;
use mbl_db::repo::items::{self, ItemDetail, ItemFilter, ItemSummary, ListQuery};
use mbl_db::repo::libraries::LibrariesRepo;
use serde::Serialize;

const SEARCH_PER_KIND: usize = 10;
const HUB_SIZE: u32 = 20;

#[derive(Debug, Clone)]
pub struct ItemService {
    db: Db,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SearchGroup {
    pub kind: MediaKind,
    pub items: Vec<ItemSummary>,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Hub {
    /// Stable identifier, e.g. `continueWatching` or `recentlyAdded.3`.
    pub key: String,
    pub title: String,
    pub library_id: Option<LibraryId>,
    pub items: Vec<ItemSummary>,
}

impl ItemService {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub async fn list(
        &self,
        actor: &Actor,
        library: LibraryId,
        query: &ListQuery,
    ) -> Result<Paginated<ItemSummary>> {
        let filter = ItemFilter::for_user(actor);
        if !filter.allows(library)
            || LibrariesRepo::new(self.db.clone()).get(library).await?.is_none()
        {
            return Err(Error::NotFound);
        }
        items::list(&self.db, &filter, library, query).await
    }

    pub async fn get(&self, actor: &Actor, id: ItemId) -> Result<ItemDetail> {
        items::detail(&self.db, &ItemFilter::for_user(actor), id).await?.ok_or(Error::NotFound)
    }

    pub async fn children(&self, actor: &Actor, id: ItemId) -> Result<Vec<ItemSummary>> {
        let filter = ItemFilter::for_user(actor);
        let parent = mbl_db::repo::media::library_of_item(&self.db, id).await?;
        if !parent.is_some_and(|lib| filter.allows(lib)) {
            return Err(Error::NotFound);
        }
        items::children(&self.db, &filter, id).await
    }

    /// Hits grouped by kind (first-seen order follows relevance), at most 10 per group.
    pub async fn search(&self, actor: &Actor, query: &str) -> Result<Vec<SearchGroup>> {
        if query.chars().count() > 200 {
            return Err(Error::Validation("query is too long".into()));
        }
        let hits =
            items::search(&self.db, &ItemFilter::for_user(actor), query, SEARCH_PER_KIND).await?;
        let mut groups: Vec<SearchGroup> = Vec::new();
        for hit in hits {
            match groups.iter_mut().find(|g| g.kind == hit.kind) {
                Some(g) => g.items.push(hit),
                None => groups.push(SearchGroup { kind: hit.kind, items: vec![hit] }),
            }
        }
        Ok(groups)
    }

    /// Continue Watching, Up Next and Recently Added per accessible library; empty hubs are
    /// left out.
    pub async fn home(&self, actor: &Actor) -> Result<Vec<Hub>> {
        let filter = ItemFilter::for_user(actor);
        let mut hubs = vec![
            Hub {
                key: "continueWatching".into(),
                title: "Continue Watching".into(),
                library_id: None,
                items: items::continue_watching(&self.db, &filter, HUB_SIZE).await?,
            },
            Hub {
                key: "upNext".into(),
                title: "Up Next".into(),
                library_id: None,
                items: items::up_next(&self.db, &filter, HUB_SIZE).await?,
            },
        ];
        for lib in LibrariesRepo::new(self.db.clone()).list().await? {
            if !filter.allows(lib.id) {
                continue;
            }
            hubs.push(Hub {
                key: format!("recentlyAdded.{}", lib.id),
                title: format!("Recently Added in {}", lib.name),
                library_id: Some(lib.id),
                items: items::recently_added(&self.db, &filter, lib.id, HUB_SIZE).await?,
            });
        }
        hubs.retain(|h| !h.items.is_empty());
        Ok(hubs)
    }
}
