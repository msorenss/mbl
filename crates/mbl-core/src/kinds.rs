use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(rename_all = "lowercase")]
pub enum LibraryKind {
    Movie,
    Show,
    Music,
    Photo,
    HomeVideo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(rename_all = "lowercase")]
pub enum MediaKind {
    Movie,
    Show,
    Season,
    Episode,
    Artist,
    Album,
    Track,
    Photo,
    Collection,
    Extra,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(rename_all = "lowercase")]
pub enum StreamKind {
    Video,
    Audio,
    Subtitle,
    Attachment,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_use_lowercase_names() {
        assert_eq!(serde_json::to_string(&LibraryKind::HomeVideo).unwrap(), "\"homevideo\"");
        assert_eq!(serde_json::to_string(&MediaKind::Episode).unwrap(), "\"episode\"");
        assert_eq!(
            serde_json::from_str::<StreamKind>("\"subtitle\"").unwrap(),
            StreamKind::Subtitle
        );
        assert!(serde_json::from_str::<MediaKind>("\"Movie\"").is_err());
    }
}
