//! DLNA media server: which libraries are shared, the browse tree and media lookup.
//!
//! DLNA has no sign-in, so everyone on the LAN sees the same content: the libraries an admin
//! picked, filtered by an optional content rating ceiling. All access goes through a synthetic
//! [`Actor`] with exactly that scope, so library ACLs and ratings are enforced by the same code
//! as for signed-in users.

use mbl_core::{
    Actor, Cursor, Error, ItemId, LibraryAccess, LibraryId, MediaKind, PartId, Result, UserId,
};
use mbl_db::Db;
use mbl_db::repo::items::{self, ItemFilter, ListQuery};
use mbl_db::repo::libraries::LibrariesRepo;
use mbl_db::repo::media;
use mbl_db::repo::settings::SettingsRepo;
use mbl_remote::dlna::{self, BrowseFlag, DidlItem, DidlRes, DlnaRendererProfile};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::stream::{ResolvedPart, StreamService};

pub const DEFAULT_NAME: &str = "Media Box Lounge";
/// Largest page returned by one Browse call.
const MAX_PAGE: u32 = 200;

/// Stored DLNA settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DlnaSettings {
    pub enabled: bool,
    pub friendly_name: String,
    pub library_ids: Vec<LibraryId>,
    pub max_rating: Option<String>,
}

impl DlnaSettings {
    /// Shared on the network: turned on with at least one library.
    pub fn is_active(&self) -> bool {
        self.enabled && !self.library_ids.is_empty()
    }

    fn actor(&self) -> Actor {
        Actor {
            user_id: UserId(0),
            is_admin: false,
            library_access: LibraryAccess::Only(self.library_ids.clone()),
            is_managed: false,
            content_rating_ceiling: self.max_rating.clone(),
        }
    }
}

/// One Browse result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowsePage {
    pub items: Vec<DidlItem>,
    pub total: u32,
}

/// Where a renderer is reached and what it is.
#[derive(Debug, Clone, Copy)]
pub struct Renderer<'a> {
    /// `http://host:port` as the renderer addressed us.
    pub base_url: &'a str,
    pub profile: DlnaRendererProfile,
}

#[derive(Debug, Clone)]
pub struct DlnaService {
    db: Db,
    stream: StreamService,
}

impl DlnaService {
    pub fn new(db: Db) -> Self {
        let stream = StreamService::new(db.clone());
        Self { db, stream }
    }

    fn settings_repo(&self) -> SettingsRepo {
        SettingsRepo::new(self.db.clone())
    }

    pub async fn settings(&self) -> Result<DlnaSettings> {
        let repo = self.settings_repo();
        let enabled = repo.get("dlna_enabled").await?.as_deref() == Some("true");
        let friendly_name = repo
            .get("dlna_friendly_name")
            .await?
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_NAME.into());
        let library_ids = repo
            .get("dlna_library_ids")
            .await?
            .unwrap_or_default()
            .split(',')
            .filter_map(|id| id.trim().parse().ok())
            .map(LibraryId)
            .collect();
        let max_rating = repo.get("dlna_max_rating").await?.filter(|r| !r.trim().is_empty());
        Ok(DlnaSettings { enabled, friendly_name, library_ids, max_rating })
    }

    /// Store which libraries are shared and the rating ceiling. Unknown library IDs are refused.
    pub async fn set_scope(
        &self,
        actor: &Actor,
        library_ids: &[LibraryId],
        max_rating: Option<&str>,
    ) -> Result<()> {
        actor.require_admin()?;
        let libraries = LibrariesRepo::new(self.db.clone()).list().await?;
        let mut ids = Vec::new();
        for id in library_ids {
            if !libraries.iter().any(|l| l.id == *id) {
                return Err(Error::Validation(format!("Library {} does not exist", id.0)));
            }
            if !ids.contains(id) {
                ids.push(*id);
            }
        }
        let joined = ids.iter().map(|id| id.0.to_string()).collect::<Vec<_>>().join(",");
        let repo = self.settings_repo();
        repo.set("dlna_library_ids", &joined).await?;
        match max_rating.map(str::trim).filter(|r| !r.is_empty()) {
            Some(rating) if rating.len() <= 16 => repo.set("dlna_max_rating", rating).await?,
            Some(_) => return Err(Error::Validation("Content rating is too long".into())),
            None => {
                repo.delete("dlna_max_rating").await?;
            }
        }
        Ok(())
    }

    /// Stable device UUID derived from the server ID.
    pub async fn udn(&self) -> Result<String> {
        let id = crate::server_id::ensure_server_id(&self.db).await?;
        let hash = Sha256::digest(format!("mbl-dlna:{id}").as_bytes());
        let h = hex(&hash[..16]);
        Ok(format!("{}-{}-{}-{}-{}", &h[..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32]))
    }

    async fn active(&self) -> Result<DlnaSettings> {
        let settings = self.settings().await?;
        if settings.is_active() { Ok(settings) } else { Err(Error::NotFound) }
    }

    /// Answer a ContentDirectory Browse. Object IDs: `0` root, `L<id>` library, `I<id>` item.
    pub async fn browse(
        &self,
        object_id: &str,
        flag: BrowseFlag,
        start: u32,
        count: u32,
        renderer: Renderer<'_>,
    ) -> Result<BrowsePage> {
        let settings = self.active().await?;
        let actor = settings.actor();
        let filter = ItemFilter::for_user(&actor);
        let count = if count == 0 { MAX_PAGE } else { count.min(MAX_PAGE) };
        let object = ObjectId::parse(object_id).ok_or(Error::NotFound)?;
        let one = |item: DidlItem| BrowsePage { items: vec![item], total: 1 };
        match (object, flag) {
            (ObjectId::Root, BrowseFlag::BrowseMetadata) => {
                let libraries = self.libraries(&settings).await?;
                Ok(one(DidlItem {
                    id: "0".into(),
                    parent_id: "-1".into(),
                    title: settings.friendly_name.clone(),
                    class: "object.container.storageFolder".into(),
                    child_count: Some(libraries.len() as u32),
                    res: None,
                }))
            }
            (ObjectId::Root, BrowseFlag::BrowseDirectChildren) => {
                let mut all = Vec::new();
                for (id, name) in self.libraries(&settings).await? {
                    let children = items::count_top_level(&self.db, &filter, id).await?;
                    all.push(library_container(id, name, children));
                }
                Ok(page(all, start, count))
            }
            (ObjectId::Library(id), flag) => {
                let name = self
                    .libraries(&settings)
                    .await?
                    .into_iter()
                    .find(|(l, _)| *l == id)
                    .map(|(_, name)| name)
                    .ok_or(Error::NotFound)?;
                let total = items::count_top_level(&self.db, &filter, id).await?;
                if flag == BrowseFlag::BrowseMetadata {
                    return Ok(one(library_container(id, name, total)));
                }
                let query = ListQuery {
                    cursor: Some(Cursor(start.to_string())),
                    limit: count,
                    ..ListQuery::default()
                };
                let listed = items::list(&self.db, &filter, id, &query).await?;
                let mut out = Vec::new();
                for item in listed.items {
                    let entry = Entry::from_summary(&item, format!("L{}", id.0));
                    if let Some(didl) = self.didl(&actor, entry, renderer).await? {
                        out.push(didl);
                    }
                }
                Ok(BrowsePage { items: out, total })
            }
            (ObjectId::Item(id), BrowseFlag::BrowseMetadata) => {
                let detail = items::detail(&self.db, &filter, id).await?.ok_or(Error::NotFound)?;
                let parent = detail
                    .parent_id
                    .map_or_else(|| format!("L{}", detail.library_id.0), |p| format!("I{}", p.0));
                let entry = Entry {
                    id: detail.id,
                    kind: detail.kind,
                    title: detail.title,
                    idx: detail.idx,
                    duration_ms: detail.duration_ms,
                    parent,
                };
                let didl = self.didl(&actor, entry, renderer).await?.ok_or(Error::NotFound)?;
                Ok(one(didl))
            }
            (ObjectId::Item(id), BrowseFlag::BrowseDirectChildren) => {
                items::detail(&self.db, &filter, id).await?.ok_or(Error::NotFound)?;
                let children = items::children(&self.db, &filter, id).await?;
                let total = children.len() as u32;
                let mut out = Vec::new();
                for child in children.iter().skip(start as usize).take(count as usize) {
                    let entry = Entry::from_summary(child, format!("I{}", id.0));
                    if let Some(didl) = self.didl(&actor, entry, renderer).await? {
                        out.push(didl);
                    }
                }
                Ok(BrowsePage { items: out, total })
            }
        }
    }

    /// Shared libraries that still exist, in the order the admin picked them.
    async fn libraries(&self, settings: &DlnaSettings) -> Result<Vec<(LibraryId, String)>> {
        let all = LibrariesRepo::new(self.db.clone()).list().await?;
        Ok(settings
            .library_ids
            .iter()
            .filter_map(|id| all.iter().find(|l| l.id == *id).map(|l| (l.id, l.name.clone())))
            .collect())
    }

    async fn didl(
        &self,
        actor: &Actor,
        entry: Entry,
        renderer: Renderer<'_>,
    ) -> Result<Option<DidlItem>> {
        let class = match entry.kind {
            MediaKind::Show | MediaKind::Season => "object.container.storageFolder",
            MediaKind::Artist => "object.container.person.musicArtist",
            MediaKind::Album => "object.container.album.musicAlbum",
            MediaKind::Movie => "object.item.videoItem.movie",
            MediaKind::Episode => "object.item.videoItem",
            MediaKind::Track => "object.item.audioItem.musicTrack",
            MediaKind::Photo => "object.item.imageItem.photo",
            MediaKind::Collection | MediaKind::Extra => return Ok(None),
        };
        let title = match (entry.kind, entry.idx) {
            (MediaKind::Episode | MediaKind::Track, Some(idx)) => format!("{idx}. {}", entry.title),
            _ => entry.title,
        };
        let mut item = DidlItem {
            id: format!("I{}", entry.id.0),
            parent_id: entry.parent,
            title,
            class: class.into(),
            child_count: None,
            res: None,
        };
        if item.is_container() {
            return Ok(Some(item));
        }
        let Some((part, size)) = self.playable_part(actor, entry.id).await? else {
            return Ok(None);
        };
        let mime = renderer.profile.mime(file_mime(&part));
        item.res = Some(DidlRes {
            url: format!("{}/dlna/media/{}.{}", renderer.base_url, part.id.0, extension(&part)),
            protocol_info: dlna::protocol_info(mime),
            size_bytes: u64::try_from(size).ok().filter(|s| *s > 0),
            duration_ms: part
                .duration_ms
                .or(entry.duration_ms)
                .and_then(|ms| u64::try_from(ms).ok())
                .filter(|ms| *ms > 0 && !mime.starts_with("image/")),
        });
        Ok(Some(item))
    }

    /// First part of the item that exists and passes the shared scope, preferring originals.
    async fn playable_part(
        &self,
        actor: &Actor,
        item: ItemId,
    ) -> Result<Option<(ResolvedPart, i64)>> {
        for part in media::parts_for_item(&self.db, item).await? {
            if part.missing_since.is_some() {
                continue;
            }
            match self.stream.resolve_part(actor, part.id).await {
                Ok(resolved) => return Ok(Some((resolved, part.size))),
                Err(Error::NotFound | Error::Forbidden | Error::Validation(_)) => continue,
                Err(e) => return Err(e),
            }
        }
        Ok(None)
    }

    /// Resolve a part for a renderer, enforcing the shared scope. Returns the part and its MIME.
    pub async fn media(
        &self,
        part_id: PartId,
        profile: DlnaRendererProfile,
    ) -> Result<(ResolvedPart, &'static str)> {
        let settings = self.active().await?;
        let part = self.stream.resolve_part(&settings.actor(), part_id).await?;
        let mime = profile.mime(file_mime(&part));
        Ok((part, mime))
    }

    /// Serve a resolved part with Range support.
    pub async fn serve(
        &self,
        part: &ResolvedPart,
        range: Option<&str>,
        head: bool,
    ) -> Result<axum::response::Response> {
        self.stream.serve_part(part, range, head).await
    }
}

/// Renderer-facing object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ObjectId {
    Root,
    Library(LibraryId),
    Item(ItemId),
}

impl ObjectId {
    fn parse(raw: &str) -> Option<Self> {
        let raw = raw.trim();
        if raw == "0" {
            return Some(Self::Root);
        }
        let (prefix, id) = raw.split_at_checked(1)?;
        let id: i64 = id.parse().ok().filter(|id| *id > 0)?;
        match prefix {
            "L" => Some(Self::Library(LibraryId(id))),
            "I" => Some(Self::Item(ItemId(id))),
            _ => None,
        }
    }
}

struct Entry {
    id: ItemId,
    kind: MediaKind,
    title: String,
    idx: Option<i64>,
    duration_ms: Option<i64>,
    parent: String,
}

impl Entry {
    fn from_summary(item: &items::ItemSummary, parent: String) -> Self {
        Self {
            id: item.id,
            kind: item.kind,
            title: item.title.clone(),
            idx: item.idx,
            duration_ms: item.duration_ms,
            parent,
        }
    }
}

fn library_container(id: LibraryId, name: String, children: u32) -> DidlItem {
    DidlItem {
        id: format!("L{}", id.0),
        parent_id: "0".into(),
        title: name,
        class: "object.container.storageFolder".into(),
        child_count: Some(children),
        res: None,
    }
}

fn page(all: Vec<DidlItem>, start: u32, count: u32) -> BrowsePage {
    let total = all.len() as u32;
    let items = all.into_iter().skip(start as usize).take(count as usize).collect();
    BrowsePage { items, total }
}

fn file_mime(part: &ResolvedPart) -> &'static str {
    mbl_stream::content_type_for_path(&part.path, part.container.as_deref())
}

/// File extension for the media URL; some renderers look at it.
fn extension(part: &ResolvedPart) -> String {
    part.path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .filter(|e| !e.is_empty() && e.len() <= 5 && e.bytes().all(|b| b.is_ascii_alphanumeric()))
        .unwrap_or_else(|| "bin".into())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_ids() {
        assert_eq!(ObjectId::parse("0"), Some(ObjectId::Root));
        assert_eq!(ObjectId::parse("L3"), Some(ObjectId::Library(LibraryId(3))));
        assert_eq!(ObjectId::parse("I42"), Some(ObjectId::Item(ItemId(42))));
        for bad in ["", "L", "L0", "I-1", "X3", "Lx", "ÅÄ"] {
            assert_eq!(ObjectId::parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn inactive_without_libraries() {
        let mut settings = DlnaSettings {
            enabled: true,
            friendly_name: DEFAULT_NAME.into(),
            library_ids: Vec::new(),
            max_rating: None,
        };
        assert!(!settings.is_active());
        settings.library_ids.push(LibraryId(1));
        assert!(settings.is_active());
        settings.enabled = false;
        assert!(!settings.is_active());
    }
}
