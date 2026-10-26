//! RFC 6238 Time-based One-Time Password (TOTP) implementation.
//!
//! Standard 6-digit codes computed via HMAC-SHA1 over 30-second steps,
//! with ±1 step (±30s) clock drift tolerance. Compatible with Google Authenticator,
//! 1Password, Bitwarden, Apple Passwords, etc.

use hmac::{Hmac, KeyInit, Mac};
use mbl_core::{Error, Result};
use sha1::Sha1;

type HmacSha1 = Hmac<Sha1>;

const STEP_SECONDS: u64 = 30;
const DIGITS_MOD: u32 = 1_000_000;
const BASE32_ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

/// Generated TOTP setup payload.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TotpSetup {
    /// Base32-encoded secret string.
    pub secret: String,
    /// `otpauth://` URI ready for QR code generation.
    pub uri: String,
}

/// Generate a new 20-byte random TOTP secret key encoded as Base32.
pub fn generate_secret() -> Result<String> {
    let mut bytes = [0u8; 20];
    getrandom::fill(&mut bytes)
        .map_err(|e| Error::Internal(format!("random source failed: {e}").into()))?;
    Ok(encode_base32(&bytes))
}

/// Construct standard `otpauth://totp/...` URI.
pub fn otpauth_uri(secret_base32: &str, username: &str, issuer: &str) -> String {
    format!(
        "otpauth://totp/{issuer}:{username}?secret={secret_base32}&issuer={issuer}&algorithm=SHA1&digits=6&period=30"
    )
}

/// Compute the 6-digit TOTP code for a secret at a given Unix timestamp in seconds.
pub fn compute_code(secret_base32: &str, timestamp_secs: u64) -> Result<String> {
    let key = decode_base32(secret_base32)
        .ok_or_else(|| Error::Validation("invalid base32 TOTP secret".into()))?;
    let counter = timestamp_secs / STEP_SECONDS;
    let code = hotp(&key, counter)?;
    Ok(format!("{code:06}"))
}

/// Verify a user-provided 6-digit TOTP code against a secret at `now_secs`,
/// checking current step, previous step (-30s), and next step (+30s) for clock drift.
pub fn verify_code(secret_base32: &str, candidate: &str, now_secs: u64) -> bool {
    let candidate = candidate.trim();
    if candidate.len() != 6 || !candidate.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }

    let Some(key) = decode_base32(secret_base32) else {
        return false;
    };

    let current_step = now_secs / STEP_SECONDS;
    let steps = [current_step, current_step.saturating_sub(1), current_step.saturating_add(1)];

    for step in steps {
        if let Ok(code) = hotp(&key, step) {
            let formatted = format!("{code:06}");
            if formatted == candidate {
                return true;
            }
        }
    }

    false
}

fn hotp(key: &[u8], counter: u64) -> Result<u32> {
    let mut mac = HmacSha1::new_from_slice(key)
        .map_err(|e| Error::Internal(format!("HMAC init failed: {e}").into()))?;
    mac.update(&counter.to_be_bytes());
    let result = mac.finalize().into_bytes();

    let offset = (result[19] & 0x0f) as usize;
    let code = ((u32::from(result[offset] & 0x7f) << 24)
        | (u32::from(result[offset + 1]) << 16)
        | (u32::from(result[offset + 2]) << 8)
        | u32::from(result[offset + 3]))
        % DIGITS_MOD;

    Ok(code)
}

/// Encode raw bytes to RFC 4648 Base32 without padding.
pub fn encode_base32(input: &[u8]) -> String {
    let mut out = String::new();
    let mut buffer: u64 = 0;
    let mut bits_left = 0;

    for &byte in input {
        buffer = (buffer << 8) | u64::from(byte);
        bits_left += 8;
        while bits_left >= 5 {
            bits_left -= 5;
            let index = ((buffer >> bits_left) & 0x1f) as usize;
            out.push(BASE32_ALPHABET[index] as char);
        }
    }

    if bits_left > 0 {
        let index = ((buffer << (5 - bits_left)) & 0x1f) as usize;
        out.push(BASE32_ALPHABET[index] as char);
    }

    out
}

/// Decode RFC 4648 Base32 string (case-insensitive, ignoring padding/spaces/dashes).
pub fn decode_base32(input: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut buffer: u64 = 0;
    let mut bits_left = 0;

    for ch in input.chars() {
        if ch.is_whitespace() || ch == '-' || ch == '=' {
            continue;
        }
        let val = match ch.to_ascii_uppercase() {
            c @ 'A'..='Z' => (c as u8 - b'A') as u64,
            c @ '2'..='7' => (c as u8 - b'2' + 26) as u64,
            _ => return None,
        };
        buffer = (buffer << 5) | val;
        bits_left += 5;
        if bits_left >= 8 {
            bits_left -= 8;
            out.push((buffer >> bits_left) as u8);
            buffer &= (1 << bits_left) - 1;
        }
    }

    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base32_roundtrip() {
        let sample = b"Hello Antigravity MBL TOTP!";
        let encoded = encode_base32(sample);
        let decoded = decode_base32(&encoded).expect("decode failed");
        assert_eq!(decoded, sample);
    }

    #[test]
    fn totp_rfc6238_test_vectors() {
        // RFC 6238 Appendix B test vector for SHA1 with secret "12345678901234567890" (ASCII)
        // In Base32: GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ
        let secret = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";

        // T0 = 59s -> step = 1 -> 8-digit 94287082 -> 6-digit 287082
        let c1 = compute_code(secret, 59).unwrap();
        assert_eq!(c1, "287082");

        // T1 = 1111111109s -> step = 37037036 -> 8-digit 07081804 -> 6-digit 081804
        let c2 = compute_code(secret, 1111111109).unwrap();
        assert_eq!(c2, "081804");

        // T2 = 1234567890s -> 8-digit 70005924 -> 6-digit 005924
        let c3 = compute_code(secret, 1234567890).unwrap();
        assert_eq!(c3, "005924");

        // Verification with drift tolerance
        assert!(verify_code(secret, "005924", 1234567890));
        assert!(verify_code(secret, "005924", 1234567890 + 20)); // same step
        assert!(verify_code(secret, "005924", 1234567890 + 35)); // next step (+1)
        assert!(verify_code(secret, "005924", 1234567890 - 25)); // prev step (-1)
        assert!(!verify_code(secret, "005924", 1234567890 + 90)); // out of window
        assert!(!verify_code(secret, "000000", 1234567890));
    }
}
