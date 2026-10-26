//! Credential primitives: password hashing (Argon2id), opaque session/API tokens,
//! and short-lived HMAC-SHA256 stream tokens.
//! Storage and policy live in `mbl-db` and `mbl-service`.

pub mod tokens;
pub mod totp;

use argon2::{Argon2, PasswordHasher, PasswordVerifier};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use mbl_core::{Error, PartId, Result, UserId, WorkerId};
use sha2::{Digest, Sha256};

/// Default lifetime of a signed direct-stream query token (6 hours).
pub const STREAM_TOKEN_TTL_MS: i64 = 6 * 60 * 60 * 1000;

/// Hash a password with Argon2id (m=19456 KiB, t=2, p=1). CPU-heavy: call from a blocking task.
pub fn hash_password(password: &str) -> Result<String> {
    Argon2::default()
        .hash_password(password.as_bytes())
        .map(|h| h.to_string())
        .map_err(|e| Error::Internal(format!("password hashing failed: {e}").into()))
}

/// Constant-time verification against a PHC string. Any malformed hash counts as a mismatch.
pub fn verify_password(password: &str, phc: &str) -> bool {
    Argon2::default().verify_password(password.as_bytes(), phc).is_ok()
}

/// Hash a 4-8 digit numeric PIN with Argon2id.
pub fn hash_pin(pin: &str) -> Result<String> {
    let trimmed = pin.trim();
    if trimmed.len() < 4 || trimmed.len() > 8 || !trimmed.chars().all(|c| c.is_ascii_digit()) {
        return Err(Error::Validation("PIN must be 4 to 8 digits".into()));
    }
    hash_password(trimmed)
}

/// Constant-time verification of a numeric PIN against an Argon2 hash.
pub fn verify_pin(pin: &str, phc: &str) -> bool {
    verify_password(pin.trim(), phc)
}

/// A valid hash of a random password, used to equalize timing for unknown usernames.
pub fn dummy_hash() -> &'static str {
    static DUMMY: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    DUMMY.get_or_init(|| new_token().and_then(|t| hash_password(&t.value)).unwrap_or_default())
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

/// Generate a random 32-byte secret key for HMAC signing.
pub fn new_secret_key() -> Result<[u8; 32]> {
    let mut key = [0u8; 32];
    getrandom::fill(&mut key)
        .map_err(|e| Error::Internal(format!("random source failed: {e}").into()))?;
    Ok(key)
}

/// SHA-256 of the token string; tokens have 256 bits of entropy so no salt/KDF is needed.
pub fn hash_token(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}

/// Claims embedded in a signed `?token=` for direct media streaming.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamTokenClaims {
    pub user_id: UserId,
    pub part_id: PartId,
    pub expires_at_ms: i64,
}

/// Sign `claims` with `secret` using HMAC-SHA256 (`<payload_b64>.<sig_b64>`).
pub fn sign_stream_token(secret: &[u8; 32], claims: StreamTokenClaims) -> String {
    let payload = format!("v1:{}:{}:{}", claims.user_id.0, claims.part_id.0, claims.expires_at_ms);
    let payload_b64 = URL_SAFE_NO_PAD.encode(payload.as_bytes());
    let mac = hmac_sha256(secret, payload_b64.as_bytes());
    let sig_b64 = URL_SAFE_NO_PAD.encode(mac);
    format!("{payload_b64}.{sig_b64}")
}

/// Verify a signed stream token against `secret`, `expected_part_id` and `now_ms`.
pub fn verify_stream_token(
    secret: &[u8; 32],
    token: &str,
    expected_part_id: PartId,
    now_ms: i64,
) -> Result<StreamTokenClaims> {
    let (payload_b64, sig_b64) = token.split_once('.').ok_or(Error::Unauthorized)?;
    let provided_sig = URL_SAFE_NO_PAD.decode(sig_b64).map_err(|_| Error::Unauthorized)?;
    let expected_sig = hmac_sha256(secret, payload_b64.as_bytes());
    if !constant_time_eq(&provided_sig, &expected_sig) {
        return Err(Error::Unauthorized);
    }

    let payload_bytes = URL_SAFE_NO_PAD.decode(payload_b64).map_err(|_| Error::Unauthorized)?;
    let payload = std::str::from_utf8(&payload_bytes).map_err(|_| Error::Unauthorized)?;
    let mut parts = payload.split(':');
    if parts.next() != Some("v1") {
        return Err(Error::Unauthorized);
    }
    let user_id =
        parts.next().and_then(|s| s.parse::<i64>().ok()).map(UserId).ok_or(Error::Unauthorized)?;
    let part_id =
        parts.next().and_then(|s| s.parse::<i64>().ok()).map(PartId).ok_or(Error::Unauthorized)?;
    let expires_at_ms =
        parts.next().and_then(|s| s.parse::<i64>().ok()).ok_or(Error::Unauthorized)?;
    if parts.next().is_some() {
        return Err(Error::Unauthorized);
    }

    if part_id != expected_part_id || expires_at_ms <= now_ms {
        return Err(Error::Unauthorized);
    }
    Ok(StreamTokenClaims { user_id, part_id, expires_at_ms })
}

/// Claims embedded in a signed `?token=` for worker media source access.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkerSourceTokenClaims {
    pub worker_id: WorkerId,
    pub part_id: PartId,
    pub expires_at_ms: i64,
}

/// Sign worker claims with `secret` using HMAC-SHA256 (`<payload_b64>.<sig_b64>`).
pub fn sign_worker_source_token(secret: &[u8; 32], claims: WorkerSourceTokenClaims) -> String {
    let payload =
        format!("w1:{}:{}:{}", claims.worker_id.0, claims.part_id.0, claims.expires_at_ms);
    let payload_b64 = URL_SAFE_NO_PAD.encode(payload.as_bytes());
    let mac = hmac_sha256(secret, payload_b64.as_bytes());
    let sig_b64 = URL_SAFE_NO_PAD.encode(mac);
    format!("{payload_b64}.{sig_b64}")
}

/// Verify a signed worker source token against `secret`, `expected_part_id` and `now_ms`.
pub fn verify_worker_source_token(
    secret: &[u8; 32],
    token: &str,
    expected_part_id: PartId,
    now_ms: i64,
) -> Result<WorkerSourceTokenClaims> {
    let (payload_b64, sig_b64) = token.split_once('.').ok_or(Error::Unauthorized)?;
    let provided_sig = URL_SAFE_NO_PAD.decode(sig_b64).map_err(|_| Error::Unauthorized)?;
    let expected_sig = hmac_sha256(secret, payload_b64.as_bytes());
    if !constant_time_eq(&provided_sig, &expected_sig) {
        return Err(Error::Unauthorized);
    }

    let payload_bytes = URL_SAFE_NO_PAD.decode(payload_b64).map_err(|_| Error::Unauthorized)?;
    let payload = std::str::from_utf8(&payload_bytes).map_err(|_| Error::Unauthorized)?;
    let mut parts = payload.split(':');
    if parts.next() != Some("w1") {
        return Err(Error::Unauthorized);
    }
    let worker_id = parts
        .next()
        .and_then(|s| s.parse::<i64>().ok())
        .map(WorkerId)
        .ok_or(Error::Unauthorized)?;
    let part_id =
        parts.next().and_then(|s| s.parse::<i64>().ok()).map(PartId).ok_or(Error::Unauthorized)?;
    let expires_at_ms =
        parts.next().and_then(|s| s.parse::<i64>().ok()).ok_or(Error::Unauthorized)?;
    if parts.next().is_some() {
        return Err(Error::Unauthorized);
    }

    if part_id != expected_part_id || expires_at_ms <= now_ms {
        return Err(Error::Unauthorized);
    }
    Ok(WorkerSourceTokenClaims { worker_id, part_id, expires_at_ms })
}

/// RFC 2104 HMAC-SHA256.
fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    const BLOCK_LEN: usize = 64;
    let mut key_block = [0u8; BLOCK_LEN];
    if key.len() > BLOCK_LEN {
        let digest = Sha256::digest(key);
        key_block[..32].copy_from_slice(&digest);
    } else {
        key_block[..key.len()].copy_from_slice(key);
    }

    let mut ipad = [0x36u8; BLOCK_LEN];
    let mut opad = [0x5cu8; BLOCK_LEN];
    for i in 0..BLOCK_LEN {
        ipad[i] ^= key_block[i];
        opad[i] ^= key_block[i];
    }

    let mut inner = Sha256::new();
    inner.update(ipad);
    inner.update(message);
    let inner_hash = inner.finalize();

    let mut outer = Sha256::new();
    outer.update(opad);
    outer.update(inner_hash);
    let outer_hash = outer.finalize();

    let mut out = [0u8; 32];
    out.copy_from_slice(&outer_hash);
    out
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (&x, &y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
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

    #[test]
    fn hmac_sha256_rfc4231_test_vector() {
        let key = [0x0bu8; 20];
        let data = b"Hi There";
        let mac = hmac_sha256(&key, data);
        let expected: [u8; 32] = [
            0xb0, 0x34, 0x4c, 0x61, 0xd8, 0xdb, 0x38, 0x53, 0x5c, 0xa8, 0xaf, 0xce, 0xaf, 0x0b,
            0xf1, 0x2b, 0x88, 0x1d, 0xc2, 0x00, 0xc9, 0x83, 0x3d, 0xa7, 0x26, 0xe9, 0x37, 0x6c,
            0x2e, 0x32, 0xcf, 0xf7,
        ];
        assert_eq!(mac, expected);
    }

    #[test]
    fn stream_token_roundtrip_and_tamper_rejection() {
        let secret = [7u8; 32];
        let claims =
            StreamTokenClaims { user_id: UserId(42), part_id: PartId(99), expires_at_ms: 10_000 };
        let token = sign_stream_token(&secret, claims);
        let verified = verify_stream_token(&secret, &token, PartId(99), 5_000).unwrap();
        assert_eq!(verified, claims);

        // Wrong part_id rejected
        assert!(verify_stream_token(&secret, &token, PartId(100), 5_000).is_err());
        // Expired rejected
        assert!(verify_stream_token(&secret, &token, PartId(99), 10_000).is_err());
        // Wrong secret rejected
        assert!(verify_stream_token(&[8u8; 32], &token, PartId(99), 5_000).is_err());
        // Tampered signature rejected
        let mut tampered = token.clone();
        tampered.push('a');
        assert!(verify_stream_token(&secret, &tampered, PartId(99), 5_000).is_err());
    }
}
