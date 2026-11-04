//! Stable per-installation server identifier used by clients for discovery and pairing.

use mbl_core::{Error, Result};
use mbl_db::Db;
use mbl_db::repo::settings::SettingsRepo;

const KEY: &str = "server_id";

/// Return the persisted server id, creating a random 128-bit hex id on first use.
pub async fn ensure_server_id(db: &Db) -> Result<String> {
    let repo = SettingsRepo::new(db.clone());
    if let Some(id) = repo.get(KEY).await?.filter(|v| is_valid(v)) {
        return Ok(id);
    }
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes)
        .map_err(|e| Error::Internal(format!("failed to generate server id: {e}").into()))?;
    let id: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    repo.set(KEY, &id).await?;
    Ok(id)
}

fn is_valid(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::is_valid;

    #[test]
    fn validates_hex_ids() {
        assert!(is_valid("0123456789abcdef0123456789abcdef"));
        assert!(!is_valid("short"));
        assert!(!is_valid("zz23456789abcdef0123456789abcdef"));
    }
}
