//! Credential primitives: password hashing (Argon2id) and opaque session/API tokens.
//! Storage and policy live in `mbl-db` and `mbl-service`.

use argon2::{Argon2, PasswordHasher, PasswordVerifier};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use mbl_core::{Error, Result};
use sha2::{Digest, Sha256};

/// Hash a password with Argon2id (m=19456 KiB, t=2, p=1). CPU-heavy: call from a blocking task.
pub fn hash_password(password: &str) -> Result<String> {
    Argon2::default()
        .hash_password(password.as_bytes())
        .map(|h| h.to_string())
        .map_err(|e| Error::Internal(format!("password hashing failed: {e}").into()))
}

/// Constant-time verification against a PHC string. Any malformed hash counts as a mismatch.
pub fn verify_password(password: &str, phc: &str) -> bool {
    Argon2::default()
        .verify_password(password.as_bytes(), phc)
        .is_ok()
}

/// A valid hash of a random password, used to equalize timing for unknown usernames.
pub fn dummy_hash() -> &'static str {
    static DUMMY: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    DUMMY.get_or_init(|| {
        new_token()
            .and_then(|t| hash_password(&t.value))
            .unwrap_or_default()
    })
}

/// A newly minted bearer token. `value` is shown to the client once; only `hash` is stored.
#[derive(Debug, Clone)]
pub struct NewToken {
    pub value: String,
    pub hash: Vec<u8>,
}

/// 32 random bytes, base64url without padding.
pub fn new_token() -> Result<NewToken> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes)
        .map_err(|e| Error::Internal(format!("random source failed: {e}").into()))?;
    let value = URL_SAFE_NO_PAD.encode(bytes);
    let hash = hash_token(&value);
    Ok(NewToken { value, hash })
}

/// SHA-256 of the token string; tokens have 256 bits of entropy so no salt/KDF is needed.
pub fn hash_token(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_roundtrip() {
        let started = std::time::Instant::now();
        let hash = hash_password("correct horse battery").unwrap();
        eprintln!("argon2id hash took {:?}", started.elapsed());
        assert!(hash.starts_with("$argon2id$v=19$m=19456,t=2,p=1$"));
        assert!(verify_password("correct horse battery", &hash));
        assert!(!verify_password("wrong", &hash));
    }

    #[test]
    fn malformed_hash_never_verifies() {
        assert!(!verify_password("x", "not-a-phc-string"));
        assert!(!verify_password("x", ""));
    }

    #[test]
    fn dummy_hash_is_parseable_and_rejects() {
        assert!(!verify_password("anything", dummy_hash()));
        assert!(argon2::PasswordHash::new(dummy_hash()).is_ok());
    }

    #[test]
    fn tokens_are_unique_and_hash_consistently() {
        let a = new_token().unwrap();
        let b = new_token().unwrap();
        assert_ne!(a.value, b.value);
        assert_eq!(a.value.len(), 43);
        assert_eq!(hash_token(&a.value), a.hash);
        assert_eq!(a.hash.len(), 32);
    }
}
