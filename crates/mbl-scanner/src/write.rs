//! Turning a probed file into items, versions, parts and streams (inside a transaction).

use mbl_core::{Error, ItemId, LibraryId, LibraryKind, MediaKind, Result, StreamKind};
use mbl_db::repo::media::{self, NewItem, NewPart, NewStream, VersionSummary};
use mbl_probe::{HdrKind, MediaInfo};
use sqlx::SqliteConnection;

use crate::walk::FoundFile;

/// What was written for one file.
#[derive(Debug, Default)]
pub struct Written {
    /// Newly created top-level items (movie/show/artist) that need metadata matching.
    pub new_top_items: Vec<ItemId>,
}

pub fn sort_title(title: &str) -> String {
    let lower = title.trim().to_lowercase();
    for article in ["the ", "a ", "an "] {
        if let Some(rest) = lower.strip_prefix(article)
            && !rest.is_empty()
        {
            return rest.to_owned();
        }
    }
    lower
}

pub fn hdr_name(h: HdrKind) -> String {
    match h {
        HdrKind::Hdr10 => "hdr10".into(),
        HdrKind::Hdr10Plus => "hdr10plus".into(),
        HdrKind::Hlg => "hlg".into(),
        HdrKind::DolbyVision { profile, .. } => format!("dv{profile}"),
    }
}

fn stream_kind(k: StreamKind) -> &'static str {
    match k {
        StreamKind::Video => "video",
        StreamKind::Audio => "audio",
        StreamKind::Subtitle => "subtitle",
        StreamKind::Attachment => "attachment",
    }
}

/// Owned strings backing a [`VersionSummary`].
pub struct Summary {
    container: String,
    video_codec: Option<String>,
    video_profile: Option<String>,
    hdr: Option<String>,
    audio_codec: Option<String>,
    bitrate: Option<i64>,
    width: Option<i64>,
    height: Option<i64>,
    bit_depth: Option<i64>,
    audio_channels: Option<i64>,
}

impl Summary {
    pub fn new(m: &MediaInfo) -> Self {
        let video = m.primary_video();
        let audio = m
            .streams_of(StreamKind::Audio)
            .find(|s| s.default)
            .or_else(|| m.streams_of(StreamKind::Audio).next());
        Self {
            container: m.container.clone(),
            video_codec: video.map(|v| v.codec.clone()),
            video_profile: video.and_then(|v| v.profile.clone()),
            hdr: video.and_then(|v| v.hdr).map(hdr_name),
            audio_codec: audio.map(|a| a.codec.clone()),
            bitrate: m.bitrate.and_then(|b| i64::try_from(b).ok()),
            width: video.and_then(|v| v.width).map(i64::from),
            height: video.and_then(|v| v.height).map(i64::from),
            bit_depth: video.and_then(|v| v.bit_depth).map(i64::from),
            audio_channels: audio.and_then(|a| a.channels).map(i64::from),
        }
    }

    pub fn as_version<'a>(&'a self, edition: Option<&'a str>) -> VersionSummary<'a> {
        VersionSummary {
            edition,
            container: Some(&self.container),
            bitrate: self.bitrate,
            width: self.width,
            height: self.height,
            video_codec: self.video_codec.as_deref(),
            video_profile: self.video_profile.as_deref(),
            bit_depth: self.bit_depth,
            hdr: self.hdr.as_deref(),
            audio_codec: self.audio_codec.as_deref(),
            audio_channels: self.audio_channels,
        }
    }
}

pub fn streams(m: &MediaInfo) -> Vec<NewStream<'_>> {
    m.streams
        .iter()
        .map(|s| NewStream {
            idx: i64::from(s.index),
            kind: stream_kind(s.kind),
            codec: &s.codec,
            profile: s.profile.as_deref(),
            language: s.language.as_deref(),
            title: s.title.as_deref(),
            is_default: s.default,
            is_forced: s.forced,
            external_path: s.external_path.as_deref(),
            width: s.width.map(i64::from),
            height: s.height.map(i64::from),
            bit_depth: s.bit_depth.map(i64::from),
            hdr: None,
            channels: s.channels.map(i64::from),
            channel_layout: s.channel_layout.as_deref(),
        })
        .collect()
}

pub struct FileCtx<'a> {
    pub library_id: LibraryId,
    pub kind: LibraryKind,
    pub file: &'a FoundFile,
    pub info: &'a MediaInfo,
    pub quick_hash: Option<&'a [u8]>,
    pub now: i64,
}

impl FileCtx<'_> {
    fn duration(&self) -> Option<i64> {
        i64::try_from(self.info.duration_ms).ok().filter(|d| *d > 0)
    }

    async fn add_part(
        &self,
        conn: &mut SqliteConnection,
        version_id: mbl_core::VersionId,
        part_index: i64,
    ) -> Result<()> {
        let probe = serde_json::to_string(self.info).map_err(Error::internal)?;
        let chapters = serde_json::to_string(&self.info.chapters).map_err(Error::internal)?;
        let part_id = media::insert_part(
            conn,
            &NewPart {
                version_id,
                path: &self.file.path,
                size: self.file.size,
                mtime: self.file.mtime,
                quick_hash: self.quick_hash,
                duration_ms: self.duration(),
                probe: &probe,
                chapters: &chapters,
                part_index,
            },
        )
        .await?;
        media::replace_streams(conn, part_id, &streams(self.info)).await
    }

    fn item<'b>(
        &self,
        kind: MediaKind,
        parent: Option<ItemId>,
        title: &'b str,
        sort: &'b str,
    ) -> NewItem<'b> {
        NewItem {
            library_id: self.library_id,
            kind: Some(kind),
            parent_id: parent,
            title,
            sort_title: sort,
            year: None,
            idx: None,
            parent_idx: None,
            originally_available_at: None,
            tmdb_id: None,
            tvdb_id: None,
            imdb_id: None,
            duration_ms: None,
        }
    }
}

/// Insert a new file. Returns `Validation` when the path cannot be understood (e.g. an episode
/// without any episode marker).
pub async fn write_new(conn: &mut SqliteConnection, ctx: &FileCtx<'_>) -> Result<Written> {
    match ctx.kind {
        LibraryKind::Movie | LibraryKind::HomeVideo => write_movie(conn, ctx).await,
        LibraryKind::Show => write_episode(conn, ctx).await,
        LibraryKind::Music => write_track(conn, ctx).await,
        LibraryKind::Photo => Ok(Written::default()),
    }
}

async fn top_item(
    conn: &mut SqliteConnection,
    ctx: &FileCtx<'_>,
    kind: MediaKind,
    item: NewItem<'_>,
    written: &mut Written,
) -> Result<ItemId> {
    if let Some(id) =
        media::find_top_item(conn, ctx.library_id, kind, item.title, item.year, item.tmdb_id)
            .await?
    {
        return Ok(id);
    }
    let id = media::insert_item(conn, &item, ctx.now).await?;
    written.new_top_items.push(id);
    Ok(id)
}

async fn child_item(
    conn: &mut SqliteConnection,
    ctx: &FileCtx<'_>,
    kind: MediaKind,
    item: NewItem<'_>,
) -> Result<ItemId> {
    let parent = item.parent_id.ok_or_else(|| Error::Validation("child without parent".into()))?;
    if let Some(id) = media::find_child(conn, parent, kind, item.idx, item.title).await? {
        return Ok(id);
    }
    media::insert_item(conn, &item, ctx.now).await
}

async fn write_movie(conn: &mut SqliteConnection, ctx: &FileCtx<'_>) -> Result<Written> {
    let rel = ctx.file.relative();
    if let Some(extra) = mbl_parser::classify_extra(rel) {
        // Extras are attached to their movie in M7.6.
        tracing::debug!(path = %ctx.file.path, ?extra, "skipping extra");
        return Ok(Written::default());
    }
    let guess = mbl_parser::parse_movie(rel);
    let stem = std::path::Path::new(&ctx.file.path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Untitled");
    let title = if guess.title.is_empty() { stem.to_owned() } else { guess.title.clone() };
    let sort = sort_title(&title);

    let mut written = Written::default();
    let mut item = ctx.item(MediaKind::Movie, None, &title, &sort);
    item.year = guess.year;
    item.tmdb_id = guess.ids.tmdb;
    item.imdb_id = guess.ids.imdb.as_deref();
    item.duration_ms = ctx.duration();
    let item_id = top_item(conn, ctx, MediaKind::Movie, item, &mut written).await?;

    let edition = guess.edition.as_deref();
    let summary = Summary::new(ctx.info);
    let existing = match guess.part {
        Some(_) => {
            let dir = std::path::Path::new(&ctx.file.path)
                .parent()
                .and_then(|p| p.to_str())
                .unwrap_or("");
            media::find_multipart_version(
                conn,
                item_id,
                edition,
                &format!("{dir}{}", std::path::MAIN_SEPARATOR),
            )
            .await?
        }
        None => None,
    };
    let version_id = match existing {
        Some(v) => v,
        None => media::insert_version(conn, item_id, &summary.as_version(edition)).await?,
    };
    ctx.add_part(conn, version_id, i64::from(guess.part.unwrap_or(0))).await?;
    Ok(written)
}

async fn write_episode(conn: &mut SqliteConnection, ctx: &FileCtx<'_>) -> Result<Written> {
    let rel = ctx.file.relative();
    if mbl_parser::classify_extra(rel).is_some() {
        return Ok(Written::default());
    }
    let g = mbl_parser::parse_episode(rel);
    if g.show.is_empty() {
        return Err(Error::Validation(format!("cannot determine show for {rel}")));
    }
    let mut episodes: Vec<u32> = g.episodes.clone();
    if episodes.is_empty()
        && let Some(abs) = g.absolute
    {
        episodes.push(abs);
    }
    if episodes.is_empty() && g.air_date.is_none() {
        return Err(Error::Validation(format!("no episode number in {rel}")));
    }

    let mut written = Written::default();
    let show_sort = sort_title(&g.show);
    let mut show = ctx.item(MediaKind::Show, None, &g.show, &show_sort);
    show.year = g.show_year;
    show.tmdb_id = g.ids.tmdb;
    show.tvdb_id = g.ids.tvdb;
    show.imdb_id = g.ids.imdb.as_deref();
    let show_id = top_item(conn, ctx, MediaKind::Show, show, &mut written).await?;

    let season_no = g.season.or_else(|| g.air_date.map(|d| d.year().unsigned_abs())).unwrap_or(1);
    let season_title =
        if season_no == 0 { "Specials".to_owned() } else { format!("Season {season_no}") };
    let season_sort = season_title.to_lowercase();
    let mut season = ctx.item(MediaKind::Season, Some(show_id), &season_title, &season_sort);
    season.idx = Some(i64::from(season_no));
    let season_id = child_item(conn, ctx, MediaKind::Season, season).await?;

    let date = g.air_date.map(|d| d.to_string());
    let summary = Summary::new(ctx.info);
    let mut version_id = None;
    let targets: Vec<Option<u32>> =
        if episodes.is_empty() { vec![None] } else { episodes.into_iter().map(Some).collect() };
    for (i, ep) in targets.into_iter().enumerate() {
        let title = match (&g.title, ep, &date) {
            (Some(t), _, _) if i == 0 => t.clone(),
            (_, Some(n), _) => format!("Episode {n}"),
            (_, None, Some(d)) => d.clone(),
            _ => "Episode".to_owned(),
        };
        let sort = title.to_lowercase();
        let mut item = ctx.item(MediaKind::Episode, Some(season_id), &title, &sort);
        item.idx = ep.map(i64::from);
        item.parent_idx = Some(i64::from(season_no));
        item.originally_available_at = date.as_deref();
        item.duration_ms = ctx.duration();
        let episode_id = child_item(conn, ctx, MediaKind::Episode, item).await?;
        match version_id {
            None => {
                let v = media::insert_version(conn, episode_id, &summary.as_version(None)).await?;
                ctx.add_part(conn, v, 0).await?;
                version_id = Some(v);
            }
            Some(v) => media::link_item_version(conn, episode_id, v).await?,
        }
    }
    Ok(written)
}

async fn write_track(conn: &mut SqliteConnection, ctx: &FileCtx<'_>) -> Result<Written> {
    let g = mbl_parser::parse_music(ctx.file.relative());
    let tags = &ctx.info.tags;
    let tag = |k: &str| tags.get(k).cloned().filter(|v| !v.is_empty());
    let leading_number =
        |v: String| v.split(['/', ' ']).next().and_then(|n| n.trim().parse::<u32>().ok());

    let artist = tag("album_artist")
        .or_else(|| tag("artist"))
        .or(g.artist)
        .unwrap_or_else(|| "Unknown Artist".into());
    let album = tag("album").or(g.album).unwrap_or_else(|| "Unknown Album".into());
    let stem = std::path::Path::new(&ctx.file.path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Track");
    let title = tag("title").or(g.title).unwrap_or_else(|| stem.to_owned());
    let track = tag("track").and_then(leading_number).or(g.track);
    let disc = tag("disc").and_then(leading_number).or(g.disc);
    let year =
        tag("date").and_then(|d| d.get(..4).and_then(|y| y.parse::<i32>().ok())).or(g.album_year);

    let mut written = Written::default();
    let artist_sort = sort_title(&artist);
    let artist_id = top_item(
        conn,
        ctx,
        MediaKind::Artist,
        ctx.item(MediaKind::Artist, None, &artist, &artist_sort),
        &mut written,
    )
    .await?;

    let album_sort = sort_title(&album);
    let mut album_item = ctx.item(MediaKind::Album, Some(artist_id), &album, &album_sort);
    album_item.year = year;
    let album_id = child_item(conn, ctx, MediaKind::Album, album_item).await?;

    let track_sort = title.to_lowercase();
    let mut track_item = ctx.item(MediaKind::Track, Some(album_id), &title, &track_sort);
    track_item.idx = track.map(i64::from);
    track_item.parent_idx = disc.map(i64::from);
    track_item.duration_ms = ctx.duration();
    // Tracks are matched by title: numbers repeat across discs.
    let track_id = match media::find_child(conn, album_id, MediaKind::Track, None, &title).await? {
        Some(id) => id,
        None => media::insert_item(conn, &track_item, ctx.now).await?,
    };

    let summary = Summary::new(ctx.info);
    let version_id = media::insert_version(conn, track_id, &summary.as_version(None)).await?;
    ctx.add_part(conn, version_id, 0).await?;
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sort_titles_drop_articles() {
        assert_eq!(sort_title("The Matrix"), "matrix");
        assert_eq!(sort_title("A Quiet Place"), "quiet place");
        assert_eq!(sort_title("An Education"), "education");
        assert_eq!(sort_title("The"), "the");
        assert_eq!(sort_title("Theory"), "theory");
    }
}
