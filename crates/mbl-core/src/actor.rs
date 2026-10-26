use serde::{Deserialize, Serialize};

use crate::{LibraryId, UserId};

/// The identity on whose behalf a service call runs. Every service method takes one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Actor {
    pub user_id: UserId,
    pub is_admin: bool,
    pub library_access: LibraryAccess,
    #[serde(default)]
    pub is_managed: bool,
    #[serde(default)]
    pub content_rating_ceiling: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LibraryAccess {
    All,
    Only(Vec<LibraryId>),
}

impl Actor {
    pub fn admin(user_id: UserId) -> Self {
        Self {
            user_id,
            is_admin: true,
            library_access: LibraryAccess::All,
            is_managed: false,
            content_rating_ceiling: None,
        }
    }

    pub fn user(user_id: UserId) -> Self {
        Self {
            user_id,
            is_admin: false,
            library_access: LibraryAccess::All,
            is_managed: false,
            content_rating_ceiling: None,
        }
    }

    pub fn require_admin(&self) -> crate::Result<()> {
        if self.is_admin { Ok(()) } else { Err(crate::Error::Forbidden) }
    }

    pub fn can_access_library(&self, id: LibraryId) -> bool {
        if self.is_admin {
            return true;
        }
        match &self.library_access {
            LibraryAccess::All => true,
            LibraryAccess::Only(ids) => ids.contains(&id),
        }
    }

    pub fn can_view_rating(&self, item_rating: Option<&str>) -> bool {
        if self.is_admin {
            return true;
        }
        let Some(ceiling) = &self.content_rating_ceiling else {
            return true;
        };
        allows_content_rating(ceiling, item_rating)
    }
}

/// Numerical strictness level for common motion picture and TV parental content ratings.
pub fn content_rating_level(rating: &str) -> u8 {
    let clean = rating.trim().to_ascii_uppercase().replace(['-', '_', ' '], "");
    match clean.as_str() {
        "G" | "TVY" | "TVG" | "U" | "ALL" | "EC" => 0,
        "PG" | "TVY7" | "TVPG" | "6" | "7" | "E" => 1,
        "PG13" | "TV14" | "12" | "14" | "T" => 2,
        "R" | "TVMA" | "15" | "16" | "18" | "M" => 3,
        "NC17" | "X" | "NR" | "UNRATED" | "AO" => 4,
        _ => 2,
    }
}

/// Check if `item_rating` is allowed under `ceiling`.
pub fn allows_content_rating(ceiling: &str, item_rating: Option<&str>) -> bool {
    let max_lvl = content_rating_level(ceiling);
    match item_rating {
        Some(r) if !r.trim().is_empty() => content_rating_level(r) <= max_lvl,
        // Items without an explicit rating inherit the default allow policy if ceiling >= PG (lvl 1)
        _ => max_lvl >= 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admin_and_library_checks() {
        let user = Actor {
            user_id: UserId(2),
            is_admin: false,
            library_access: LibraryAccess::Only(vec![LibraryId(1)]),
            is_managed: false,
            content_rating_ceiling: None,
        };
        assert!(user.require_admin().is_err());
        assert!(user.can_access_library(LibraryId(1)));
        assert!(!user.can_access_library(LibraryId(2)));

        let admin = Actor::admin(UserId(1));
        assert!(admin.require_admin().is_ok());
        assert!(admin.can_access_library(LibraryId(99)));
    }

    #[test]
    fn content_rating_ceilings_m6_1() {
        let kid = Actor {
            user_id: UserId(3),
            is_admin: false,
            library_access: LibraryAccess::All,
            is_managed: true,
            content_rating_ceiling: Some("PG".into()),
        };
        assert!(kid.can_view_rating(Some("G")));
        assert!(kid.can_view_rating(Some("TV-Y7")));
        assert!(kid.can_view_rating(Some("PG")));
        assert!(!kid.can_view_rating(Some("PG-13")));
        assert!(!kid.can_view_rating(Some("R")));
        assert!(!kid.can_view_rating(Some("TV-MA")));

        let teen = Actor {
            user_id: UserId(4),
            is_admin: false,
            library_access: LibraryAccess::All,
            is_managed: true,
            content_rating_ceiling: Some("PG-13".into()),
        };
        assert!(teen.can_view_rating(Some("PG")));
        assert!(teen.can_view_rating(Some("PG-13")));
        assert!(teen.can_view_rating(Some("TV-14")));
        assert!(!teen.can_view_rating(Some("R")));
        assert!(!teen.can_view_rating(Some("TV-MA")));
    }
}
