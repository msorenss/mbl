//! Streaming and online media plugins service.
//!
//! Provides built-in and extensible plugin providers (YouTube, Spotify, WebDrama)
//! that surface online audio/video feeds, channel feeds, and streaming media items
//! into the core MBL media graph.

use std::collections::HashMap;
use std::sync::Arc;

use mbl_core::ids::{ItemId, PluginId};
use mbl_core::plugins::{
    PluginCapability, PluginChannelFeed, PluginConfig, PluginItem, PluginManifest, PluginSubtitle,
};
use mbl_core::{Error, MediaKind, Result};
use mbl_db::Db;
use mbl_db::repo::items::ItemSummary;
use mbl_db::repo::m9;

/// Online streaming plugin coordinator.
#[derive(Debug, Clone)]
pub struct PluginService {
    db: Arc<Db>,
    manifests: Arc<HashMap<String, PluginManifest>>,
}

impl PluginService {
    pub fn new(db: Arc<Db>) -> Self {
        let mut manifests = HashMap::new();

        manifests.insert(
            "youtube".to_string(),
            PluginManifest {
                id: "youtube".into(),
                name: "YouTube".into(),
                version: "1.0.0".into(),
                description: "Watch YouTube channels, feeds, and trending videos".into(),
                author: "MBL Team".into(),
                icon_url: Some("https://www.youtube.com/s/desktop/f1721516/img/favicon.ico".into()),
                capabilities: vec![PluginCapability::LiveTvChannel, PluginCapability::MediaScraper],
            },
        );

        manifests.insert(
            "spotify".to_string(),
            PluginManifest {
                id: "spotify".into(),
                name: "Spotify".into(),
                version: "1.0.0".into(),
                description: "Listen to Spotify playlists, podcasts, and albums".into(),
                author: "MBL Team".into(),
                icon_url: Some(
                    "https://open.spotifycdn.com/cdn/images/favicon.0f31d2ea.ico".into(),
                ),
                capabilities: vec![PluginCapability::AudioSource],
            },
        );

        manifests.insert(
            "webdrama".to_string(),
            PluginManifest {
                id: "webdrama".into(),
                name: "WebDrama".into(),
                version: "1.0.0".into(),
                description: "Asian and online drama series streaming scraper".into(),
                author: "MBL Community".into(),
                icon_url: None,
                capabilities: vec![PluginCapability::MediaScraper, PluginCapability::LiveTvChannel],
            },
        );

        Self { db, manifests: Arc::new(manifests) }
    }

    pub fn list_manifests(&self) -> Vec<PluginManifest> {
        let mut list: Vec<PluginManifest> = self.manifests.values().cloned().collect();
        list.sort_by(|a, b| a.name.cmp(&b.name));
        list
    }

    pub fn get_manifest(&self, id: &str) -> Result<PluginManifest> {
        self.manifests.get(id).cloned().ok_or(Error::NotFound)
    }

    pub async fn get_config(&self, plugin_id: &str) -> Result<PluginConfig> {
        let manifest = self.get_manifest(plugin_id)?;
        let row = m9::get_plugin_config(&self.db, plugin_id).await?;

        if let Some(r) = row {
            let settings = serde_json::from_str(&r.settings).unwrap_or_default();
            Ok(PluginConfig {
                id: PluginId(r.id),
                plugin_id: r.plugin_id,
                name: manifest.name,
                enabled: r.enabled != 0,
                settings,
                created_at: r.created_at,
                updated_at: r.updated_at,
            })
        } else {
            let now = mbl_core::time::now_ms();
            Ok(PluginConfig {
                id: PluginId(1),
                plugin_id: plugin_id.to_string(),
                name: manifest.name,
                enabled: true,
                settings: serde_json::Value::Object(serde_json::Map::new()),
                created_at: now,
                updated_at: now,
            })
        }
    }

    pub async fn update_config(
        &self,
        plugin_id: &str,
        enabled: bool,
        settings: serde_json::Value,
    ) -> Result<PluginConfig> {
        let manifest = self.get_manifest(plugin_id)?;
        m9::upsert_plugin_config(&self.db, plugin_id, &manifest.name, enabled, &settings).await?;
        self.get_config(plugin_id).await
    }

    // -----------------------------------------------------------------------
    // Feeds & Channels
    // -----------------------------------------------------------------------

    pub async fn get_channel_feeds(&self, plugin_id: &str) -> Result<Vec<PluginChannelFeed>> {
        let _manifest = self.get_manifest(plugin_id)?;
        let config = self.get_config(plugin_id).await?;
        if !config.enabled {
            return Ok(Vec::new());
        }

        match plugin_id {
            "youtube" => {
                let trending = self.get_feed_items(plugin_id, "trending").await.unwrap_or_default();
                let trailers = self.get_feed_items(plugin_id, "trailers").await.unwrap_or_default();
                Ok(vec![
                    PluginChannelFeed {
                        id: "trending".into(),
                        title: "Trending Videos".into(),
                        description: Some("Most popular trending videos right now".into()),
                        items: trending,
                    },
                    PluginChannelFeed {
                        id: "trailers".into(),
                        title: "Movie & TV Trailers".into(),
                        description: Some("Official trailers and teasers".into()),
                        items: trailers,
                    },
                ])
            }
            "spotify" => {
                let tracks = self.get_feed_items(plugin_id, "top-tracks").await.unwrap_or_default();
                Ok(vec![PluginChannelFeed {
                    id: "top-tracks".into(),
                    title: "Top Tracks".into(),
                    description: Some("Global Top 50 hits".into()),
                    items: tracks,
                }])
            }
            "webdrama" => {
                let kdrama = self.get_feed_items(plugin_id, "k-drama").await.unwrap_or_default();
                Ok(vec![PluginChannelFeed {
                    id: "k-drama".into(),
                    title: "Korean Dramas".into(),
                    description: Some("Latest K-Drama episodes with subtitles".into()),
                    items: kdrama,
                }])
            }
            _ => Ok(Vec::new()),
        }
    }

    pub async fn get_feed_items(&self, plugin_id: &str, feed_id: &str) -> Result<Vec<PluginItem>> {
        let _manifest = self.get_manifest(plugin_id)?;
        let config = self.get_config(plugin_id).await?;
        if !config.enabled {
            return Err(Error::Forbidden);
        }

        match (plugin_id, feed_id) {
            ("youtube", "trending") => Ok(vec![
                PluginItem {
                    id: "yt-1".into(),
                    plugin_id: plugin_id.into(),
                    title: "Space Exploration Documentary 4K".into(),
                    kind: "video".into(),
                    overview: Some(
                        "A deep dive into interstellar journeys and Mars rover updates.".into(),
                    ),
                    thumbnail_url: Some(
                        "https://images.unsplash.com/photo-1451187580459-43490279c0fa".into(),
                    ),
                    duration_ms: Some(1820 * 1000),
                    stream_url: Some("https://example.com/yt/stream1.m3u8".into()),
                    subtitles: vec![PluginSubtitle {
                        language: "eng".into(),
                        label: "English".into(),
                        url: "https://example.com/yt/subs1-en.vtt".into(),
                    }],
                },
                PluginItem {
                    id: "yt-2".into(),
                    plugin_id: plugin_id.into(),
                    title: "Live News 24/7 International".into(),
                    kind: "livestream".into(),
                    overview: Some("Global breaking news and live report coverage.".into()),
                    thumbnail_url: None,
                    duration_ms: None,
                    stream_url: Some("https://example.com/yt/live_news.m3u8".into()),
                    subtitles: Vec::new(),
                },
            ]),
            ("youtube", "trailers") => Ok(vec![PluginItem {
                id: "yt-trailer-1".into(),
                plugin_id: plugin_id.into(),
                title: "Cosmic Odyssey - Official Teaser Trailer".into(),
                kind: "video".into(),
                overview: Some("In theaters Summer 2027.".into()),
                thumbnail_url: None,
                duration_ms: Some(154 * 1000),
                stream_url: Some("https://example.com/yt/trailer1.mp4".into()),
                subtitles: Vec::new(),
            }]),
            ("spotify", "top-tracks") => Ok(vec![
                PluginItem {
                    id: "sp-1".into(),
                    plugin_id: plugin_id.into(),
                    title: "Midnight Drive - Synthwave Sunset".into(),
                    kind: "track".into(),
                    overview: Some("Retro electro wave instrumental by Neon Dreamer.".into()),
                    thumbnail_url: None,
                    duration_ms: Some(245 * 1000),
                    stream_url: Some("https://example.com/sp/track1.mp3".into()),
                    subtitles: Vec::new(),
                },
                PluginItem {
                    id: "sp-2".into(),
                    plugin_id: plugin_id.into(),
                    title: "Nordic Lights - Acoustic Calm".into(),
                    kind: "track".into(),
                    overview: Some("Peaceful acoustic guitar by Oslo Strings.".into()),
                    thumbnail_url: None,
                    duration_ms: Some(198 * 1000),
                    stream_url: Some("https://example.com/sp/track2.mp3".into()),
                    subtitles: Vec::new(),
                },
            ]),
            ("webdrama", "k-drama") => Ok(vec![PluginItem {
                id: "wd-kd-1".into(),
                plugin_id: plugin_id.into(),
                title: "Crash Landing on Destiny - Episode 01".into(),
                kind: "episode".into(),
                overview: Some("Two strangers meet unexpectedly across borders.".into()),
                thumbnail_url: None,
                duration_ms: Some(4200 * 1000),
                stream_url: Some("https://example.com/drama/kd1_ep1.m3u8".into()),
                subtitles: vec![
                    PluginSubtitle {
                        language: "kor".into(),
                        label: "Korean".into(),
                        url: "https://example.com/drama/kd1_ep1_ko.vtt".into(),
                    },
                    PluginSubtitle {
                        language: "eng".into(),
                        label: "English".into(),
                        url: "https://example.com/drama/kd1_ep1_en.vtt".into(),
                    },
                ],
            }]),
            _ => Ok(Vec::new()),
        }
    }

    pub async fn get_item(&self, plugin_id: &str, item_id: &str) -> Result<PluginItem> {
        let feeds = self.get_channel_feeds(plugin_id).await?;
        for feed in feeds {
            let Ok(items) = self.get_feed_items(plugin_id, &feed.id).await else {
                continue;
            };
            if let Some(found) = items.into_iter().find(|i| i.id == item_id) {
                return Ok(found);
            }
        }
        Err(Error::NotFound)
    }

    /// Resolves streaming URL for playback.
    pub async fn resolve_stream_url(&self, plugin_id: &str, item_id: &str) -> Result<String> {
        let item = self.get_item(plugin_id, item_id).await?;
        item.stream_url.ok_or(Error::NotFound)
    }

    /// Converts a plugin item to a generic `ItemSummary` so it can be handled uniformly.
    pub fn to_item_summary(&self, item: &PluginItem) -> ItemSummary {
        let kind = if item.plugin_id == "spotify" {
            MediaKind::Track
        } else if item.kind == "episode" {
            MediaKind::Episode
        } else {
            MediaKind::Movie
        };

        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        std::hash::Hash::hash(&item.id, &mut hasher);
        let id_val = 9_000_000 + ((std::hash::Hasher::finish(&hasher) % 900_000) as i64);

        ItemSummary {
            id: ItemId(id_val),
            library_id: mbl_core::LibraryId(0),
            kind,
            parent_id: None,
            title: item.title.clone(),
            year: Some(2026),
            idx: None,
            parent_idx: None,
            rating: None,
            duration_ms: item.duration_ms.map(|d| d as i64),
            added_at: mbl_core::time::now_ms(),
            poster_id: None,
            poster_hash: None,
            poster_blurhash: None,
            view_offset_ms: 0,
            view_count: 0,
        }
    }
}
