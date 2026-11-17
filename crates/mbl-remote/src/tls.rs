//! Local certificate authority and certificate helpers for HTTPS.
//!
//! MBL can serve HTTPS using either a self-managed local CA (the root certificate is installed once
//! on client devices) or a certificate fetched from the local Tailscale daemon. This module holds the
//! pure, testable pieces: CA/server certificate generation and PEM extraction.

use mbl_core::{Error, Result};
use rcgen::{
    BasicConstraints, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
    KeyUsagePurpose,
};
use time::{Duration, OffsetDateTime};

const CA_COMMON_NAME: &str = "MBL Local CA";
const CA_VALIDITY_DAYS: i64 = 3650;
const SERVER_VALIDITY_DAYS: i64 = 365;

/// A PEM certificate with its private key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PemPair {
    pub cert_pem: String,
    pub key_pem: String,
    /// Expiry as unix seconds.
    pub not_after: i64,
}

fn tls_err(context: &str, error: impl std::fmt::Display) -> Error {
    Error::Internal(format!("{context}: {error}").into())
}

fn ca_params(not_before: OffsetDateTime) -> Result<CertificateParams> {
    let mut params =
        CertificateParams::new(Vec::<String>::new()).map_err(|e| tls_err("CA params", e))?;
    params.distinguished_name.push(DnType::CommonName, CA_COMMON_NAME);
    params.distinguished_name.push(DnType::OrganizationName, "Media Box Lounge");
    // Path length 0: the root may sign server certificates but never another CA, so a leaked
    // server key or a mis-issued certificate cannot be turned into a signing authority.
    params.is_ca = IsCa::Ca(BasicConstraints::Constrained(0));
    params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    params.use_authority_key_identifier_extension = true;
    params.not_before = not_before;
    params.not_after = not_before + Duration::days(CA_VALIDITY_DAYS);
    Ok(params)
}

/// Create a new self-signed root CA valid for ten years.
pub fn generate_local_ca() -> Result<PemPair> {
    let now = OffsetDateTime::now_utc();
    let params = ca_params(now - Duration::days(1))?;
    let key = KeyPair::generate().map_err(|e| tls_err("CA key", e))?;
    let cert = params.self_signed(&key).map_err(|e| tls_err("CA certificate", e))?;
    Ok(PemPair {
        cert_pem: cert.pem(),
        key_pem: key.serialize_pem(),
        not_after: (now + Duration::days(CA_VALIDITY_DAYS - 1)).unix_timestamp(),
    })
}

/// Most names a single certificate may carry.
const MAX_NAMES: usize = 32;

/// A host is an IP address or a DNS name made of valid labels. A wildcard is accepted only as
/// the whole left-most label (`*.example.lan`).
fn valid_host(name: &str) -> bool {
    if name.parse::<std::net::IpAddr>().is_ok() {
        return true;
    }
    let rest = name.strip_prefix("*.").unwrap_or(name);
    !rest.is_empty()
        && name.len() <= 253
        && rest.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
        })
}

/// Normalise requested host names: trim, drop empties and duplicates, reject anything that is
/// not a DNS name or IP address.
pub fn clean_names(names: &[String]) -> Result<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    for raw in names {
        let name = raw.trim().trim_end_matches('.').to_ascii_lowercase();
        if name.is_empty() || out.contains(&name) {
            continue;
        }
        if !valid_host(&name) {
            return Err(Error::Validation(format!("invalid host name: {}", raw.trim())));
        }
        out.push(name);
    }
    if out.is_empty() {
        return Err(Error::Validation("at least one host name or IP address is required".into()));
    }
    if out.len() > MAX_NAMES {
        return Err(Error::Validation(format!("at most {MAX_NAMES} host names are allowed")));
    }
    Ok(out)
}

/// Issue a server certificate for `names` (DNS names and/or IP addresses) signed by the CA.
///
/// The CA is rebuilt from its key and fixed distinguished name, so only the CA private key needs to
/// be stored; the issued chain still validates against the originally installed root certificate.
pub fn issue_server_cert(ca_key_pem: &str, names: &[String]) -> Result<PemPair> {
    let names = clean_names(names)?;
    let ca_key = KeyPair::from_pem(ca_key_pem).map_err(|e| tls_err("CA key", e))?;
    let issuer = Issuer::new(ca_params(OffsetDateTime::now_utc())?, ca_key);

    let now = OffsetDateTime::now_utc();
    let mut params =
        CertificateParams::new(names.clone()).map_err(|e| tls_err("server params", e))?;
    params.distinguished_name.push(DnType::CommonName, names[0].clone());
    params.is_ca = IsCa::NoCa;
    params.key_usages = vec![KeyUsagePurpose::DigitalSignature, KeyUsagePurpose::KeyEncipherment];
    params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    params.use_authority_key_identifier_extension = true;
    params.not_before = now - Duration::days(1);
    params.not_after = now + Duration::days(SERVER_VALIDITY_DAYS);
    let key = KeyPair::generate().map_err(|e| tls_err("server key", e))?;
    let cert = params.signed_by(&key, &issuer).map_err(|e| tls_err("server certificate", e))?;
    Ok(PemPair {
        cert_pem: cert.pem(),
        key_pem: key.serialize_pem(),
        not_after: (now + Duration::days(SERVER_VALIDITY_DAYS)).unix_timestamp(),
    })
}

fn extract_blocks(text: &str, label: &str) -> Vec<String> {
    let begin = format!("-----BEGIN {label}-----");
    let end = format!("-----END {label}-----");
    let mut blocks = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find(&begin) {
        let Some(stop_rel) = rest[start..].find(&end) else { break };
        let stop = start + stop_rel + end.len();
        blocks.push(format!("{}\n", rest[start..stop].replace("\r\n", "\n")));
        rest = &rest[stop..];
    }
    blocks
}

/// Split Tailscale's `type=pair` response into `(certificate chain PEM, private key PEM)`.
pub fn split_pair(text: &str) -> Result<(String, String)> {
    let certs = extract_blocks(text, "CERTIFICATE");
    let key = ["PRIVATE KEY", "EC PRIVATE KEY", "RSA PRIVATE KEY"]
        .iter()
        .find_map(|label| extract_blocks(text, label).into_iter().next());
    match (certs.is_empty(), key) {
        (false, Some(key)) => Ok((certs.concat(), key)),
        _ => Err(Error::Unavailable(
            "Tailscale did not return a certificate (enable HTTPS certificates in the tailnet admin console)"
                .into(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ca_and_server_cert_round_trip() {
        let ca = generate_local_ca().expect("ca");
        assert!(ca.cert_pem.contains("BEGIN CERTIFICATE"));
        let server = issue_server_cert(&ca.key_pem, &["mbl.local".into(), "192.168.1.10".into()])
            .expect("server cert");
        assert!(server.cert_pem.contains("BEGIN CERTIFICATE"));
        assert!(server.key_pem.contains("PRIVATE KEY"));
        assert!(server.not_after > OffsetDateTime::now_utc().unix_timestamp());
    }

    #[test]
    fn rejects_bad_names() {
        assert!(clean_names(&[]).is_err());
        for bad in [
            "bad name!",
            "..",
            "a..b",
            "*",
            "a*b.lan",
            "*.*.lan",
            "-a.lan",
            "a-.lan",
            ":::",
            "[::1]",
        ] {
            assert!(clean_names(&[bad.into()]).is_err(), "{bad} should be rejected");
        }
        assert_eq!(
            clean_names(&["*.Home.Lan.".into(), "::1".into(), "fd7a::1".into()]).expect("names"),
            vec!["*.home.lan".to_string(), "::1".to_string(), "fd7a::1".to_string()]
        );
        let many: Vec<String> = (0..=MAX_NAMES).map(|i| format!("host{i}.lan")).collect();
        assert!(clean_names(&many).is_err());
        assert_eq!(
            clean_names(&[" Host ".into(), "host".into(), "10.0.0.1".into()]).expect("names"),
            vec!["host".to_string(), "10.0.0.1".to_string()]
        );
    }

    #[test]
    fn splits_tailscale_pair() {
        let pem = "-----BEGIN PRIVATE KEY-----\nAAAA\n-----END PRIVATE KEY-----\n-----BEGIN CERTIFICATE-----\nBBBB\n-----END CERTIFICATE-----\n";
        let (cert, key) = split_pair(pem).expect("pair");
        assert!(cert.contains("BBBB") && key.contains("AAAA"));
        assert!(split_pair("not found").is_err());
    }
}
