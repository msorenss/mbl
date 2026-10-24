use serde::{Deserialize, Serialize};

use crate::{LibraryId, UserId};

/// The identity on whose behalf a service call runs. Every service method takes one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Actor {
    pub user_id: UserId,
    pub is_admin: bool,
    pub library_access: LibraryAccess,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LibraryAccess {
    All,
    Only(Vec<LibraryId>),
}

impl Actor {
    pub fn require_admin(&self) -> crate::Result<()> {
        if self.is_admin { Ok(()) } else { Err(crate::Error::Forbidden) }
    }

    pub fn can_access_library(&self, id: LibraryId) -> bool {
        match &self.library_access {
            LibraryAccess::All => true,
            LibraryAccess::Only(ids) => ids.contains(&id),
        }
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
        };
        assert!(user.require_admin().is_err());
        assert!(user.can_access_library(LibraryId(1)));
        assert!(!user.can_access_library(LibraryId(2)));

        let admin =
            Actor { user_id: UserId(1), is_admin: true, library_access: LibraryAccess::All };
        assert!(admin.require_admin().is_ok());
        assert!(admin.can_access_library(LibraryId(99)));
    }
}
