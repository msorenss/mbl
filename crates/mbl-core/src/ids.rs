//! Typed row IDs. All IDs are SQLite `INTEGER PRIMARY KEY` values wrapped in newtypes.

macro_rules! id_newtype {
    ($($name:ident),* $(,)?) => { $(
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord,
            serde::Serialize, serde::Deserialize, sqlx::Type,
        )]
        #[serde(transparent)]
        #[sqlx(transparent)]
        pub struct $name(pub i64);

        impl From<i64> for $name {
            fn from(v: i64) -> Self { Self(v) }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.fmt(f)
            }
        }
    )* };
}

id_newtype!(UserId, LibraryId, ItemId, VersionId, PartId, StreamId, ArtworkId, JobId, SessionId,);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_serialize_as_plain_numbers() {
        assert_eq!(serde_json::to_string(&ItemId(42)).unwrap(), "42");
        assert_eq!(serde_json::from_str::<UserId>("7").unwrap(), UserId(7));
        assert_eq!(LibraryId::from(3).to_string(), "3");
    }
}
