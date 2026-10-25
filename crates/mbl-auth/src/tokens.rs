//! Scoped API tokens (`api_tokens`), scope validation, and network allowlists.

use std::collections::BTreeSet;
use std::str::FromStr;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use mbl_core::{Error, Result};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{NewToken, hash_token};

/// Fine-grained capability scope for API / MCP tokens (Appendix A.10).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, ToSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    Read,
    Playback,
    Library,
    Users,
    Remote,
    LiveTv,
    Admin,
}

impl Scope {
    pub const ALL: [Self; 7] = [
        Self::Read,
        Self::Playback,
        Self::Library,
        Self::Users,
        Self::Remote,
        Self::LiveTv,
        Self::Admin,
    ];

    pub const NON_ADMIN_ALLOWED: [Self; 4] =
        [Self::Read, Self::Playback, Self::Library, Self::LiveTv];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Playback => "playback",
            Self::Library => "library",
            Self::Users => "users",
            Self::Remote => "remote",
            Self::LiveTv => "livetv",
            Self::Admin => "admin",
        }
    }

    /// True if only an administrator may hold or mint a token with this scope.
    pub fn is_admin_only(self) -> bool {
        matches!(self, Self::Admin | Self::Users | Self::Remote)
    }
}

impl FromStr for Scope {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "read" => Ok(Self::Read),
            "playback" => Ok(Self::Playback),
            "library" => Ok(Self::Library),
            "users" => Ok(Self::Users),
            "remote" => Ok(Self::Remote),
            "livetv" => Ok(Self::LiveTv),
            "admin" => Ok(Self::Admin),
            other => Err(Error::Validation(format!("unknown token scope: '{other}'"))),
        }
    }
}

/// Set of [`Scope`]s granted to a caller or token.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ScopeSet(pub BTreeSet<Scope>);

impl ScopeSet {
    pub fn new(scopes: impl IntoIterator<Item = Scope>) -> Self {
        Self(scopes.into_iter().collect())
    }

    /// Full scopes available to a user based on whether they are an administrator.
    pub fn full_for_user(is_admin: bool) -> Self {
        if is_admin { Self::new(Scope::ALL) } else { Self::new(Scope::NON_ADMIN_ALLOWED) }
    }

    /// Parse and validate a list of scope names.
    pub fn parse_list(names: &[String]) -> Result<Self> {
        if names.is_empty() {
            return Err(Error::Validation("at least one scope is required".into()));
        }
        let mut set = BTreeSet::new();
        for name in names {
            set.insert(name.parse::<Scope>()?);
        }
        Ok(Self(set))
    }

    /// Ensure the requested scopes do not exceed the user's own rights.
    /// Non-admins cannot obtain `admin`, `users`, or `remote`.
    pub fn validate_for_user(&self, is_admin: bool) -> Result<()> {
        if !is_admin && self.0.iter().any(|s| s.is_admin_only()) {
            return Err(Error::Forbidden);
        }
        Ok(())
    }

    /// Cap this scope set to the user's own rights (defense-in-depth if user role changed after token creation).
    pub fn capped_for_user(&self, is_admin: bool) -> Self {
        if is_admin {
            self.clone()
        } else {
            Self(self.0.iter().copied().filter(|s| !s.is_admin_only()).collect())
        }
    }

    /// Check whether this scope set grants `required`. `Scope::Admin` implies all scopes.
    pub fn allows(&self, required: Scope) -> bool {
        self.0.contains(&required) || self.0.contains(&Scope::Admin)
    }

    pub fn to_strings(&self) -> Vec<String> {
        self.0.iter().map(|s| s.as_str().to_owned()).collect()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Validate and normalize `allowed_networks` (`"lan"`, `"tailnet"`, `"wan"`).
/// Defaults to `["lan", "tailnet"]` when `None`.
pub fn validate_allowed_networks(networks: Option<&[String]>) -> Result<Vec<String>> {
    let Some(list) = networks else {
        return Ok(vec!["lan".to_owned(), "tailnet".to_owned()]);
    };
    if list.is_empty() {
        return Err(Error::Validation("allowedNetworks cannot be empty".into()));
    }
    let mut out = Vec::new();
    for item in list {
        let norm = item.trim().to_ascii_lowercase();
        if !matches!(norm.as_str(), "lan" | "tailnet" | "wan") {
            return Err(Error::Validation(format!(
                "invalid network in allowedNetworks: '{item}' (expected 'lan', 'tailnet', or 'wan')"
            )));
        }
        if !out.contains(&norm) {
            out.push(norm);
        }
    }
    Ok(out)
}

/// Mint a new `mbl_`-prefixed API token (256 bits of entropy) and its SHA-256 hash.
pub fn new_api_token() -> Result<NewToken> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes)
        .map_err(|e| Error::Internal(format!("random source failed: {e}").into()))?;
    let value = format!("mbl_{}", URL_SAFE_NO_PAD.encode(bytes));
    let hash = hash_token(&value);
    Ok(NewToken { value, hash })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_parsing_and_non_admin_cap() {
        let scopes = ScopeSet::parse_list(&["read".into(), "library".into()]).unwrap();
        assert!(scopes.allows(Scope::Read));
        assert!(scopes.allows(Scope::Library));
        assert!(!scopes.allows(Scope::Playback));
        assert!(scopes.validate_for_user(false).is_ok());

        for forbidden in ["admin", "users", "remote"] {
            let s = ScopeSet::parse_list(&[forbidden.to_owned()]).unwrap();
            assert!(matches!(s.validate_for_user(false), Err(Error::Forbidden)));
            assert!(s.validate_for_user(true).is_ok());
        }
    }

    #[test]
    fn admin_scope_implies_all() {
        let s = ScopeSet::new([Scope::Admin]);
        for scope in Scope::ALL {
            assert!(s.allows(scope));
        }
    }

    #[test]
    fn api_token_prefix_and_hash() {
        let tok = new_api_token().unwrap();
        assert!(tok.value.starts_with("mbl_"));
        assert_eq!(hash_token(&tok.value), tok.hash);
    }
}
