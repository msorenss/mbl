//! Durable certificate requests and automatic renewal for the server.
use axum::{
    Router,
    extract::{Path, State},
    http::StatusCode,
    routing::get,
};
use instant_acme::{
    Account, AuthorizationStatus, ChallengeType, Identifier, LetsEncrypt, NewAccount, NewOrder,
    OrderStatus, RetryPolicy,
};
use mbl_core::{Error, Result};
use mbl_db::{Db, repo::settings::SettingsRepo};
use mbl_remote::{
    TailscaleClient,
    tls::{clean_names, generate_local_ca, issue_server_cert, split_pair},
};
use serde::{Deserialize, Serialize};
use std::future::IntoFuture;
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
    time::Duration,
};

pub const CHALLENGE_PORT: u16 = 8081;
static MUTATION: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
/// Renew a certificate once it has less than this many seconds left.
const RENEW_BEFORE: i64 = 30 * 86400;
/// Wait this long before trying again after a failed issuance or renewal.
const RETRY_AFTER: i64 = 3600;

/// Longest a request waits for another certificate change to finish.
const MUTATION_WAIT: Duration = Duration::from_secs(5);

/// Take the mutation lock. Local changes finish in milliseconds, so a short wait is enough to
/// queue behind them; an ACME or Tailscale issuance can hold the lock for minutes, and then the
/// caller is told to come back instead of hanging.
pub async fn lock_mutation() -> Result<tokio::sync::MutexGuard<'static, ()>> {
    tokio::time::timeout(MUTATION_WAIT, MUTATION.lock()).await.map_err(|_| {
        Error::Unavailable(
            "A certificate request is already running. Try again when it has finished.".into(),
        )
    })
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Request {
    pub mode: String,
    pub domain: String,
    pub email: String,
}

pub fn now() -> i64 {
    time::OffsetDateTime::now_utc().unix_timestamp()
}

pub fn domain_name(raw: &str) -> Result<String> {
    let name = raw.trim().trim_end_matches('.').to_ascii_lowercase();
    if name.len() > 253
        || !name.contains('.')
        || name.ends_with(".local")
        || name.parse::<std::net::IpAddr>().is_ok()
        || name.split('.').any(|label| {
            label.is_empty()
                || label.len() > 63
                || label.starts_with('-')
                || label.ends_with('-')
                || !label.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
        })
    {
        return Err(Error::Validation(
            "Enter a public DNS hostname without https://, port or path".into(),
        ));
    }
    Ok(name)
}

/// Validate the key pair, requested DNS name and validity before activation.
pub fn expiry(cert: &str, key: &str, domain: &str) -> Result<i64> {
    use x509_parser::{extensions::GeneralName, parse_x509_certificate};
    let fail = || {
        Error::Validation(
            "Certificate/key mismatch, invalid certificate, or hostname not covered".into(),
        )
    };
    let chain = rustls_pemfile::certs(&mut cert.as_bytes())
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|_| fail())?;
    let private =
        rustls_pemfile::private_key(&mut key.as_bytes()).map_err(|_| fail())?.ok_or_else(fail)?;
    let provider = rustls::crypto::aws_lc_rs::default_provider();
    rustls::sign::CertifiedKey::from_der(chain.clone(), private, &provider).map_err(|_| fail())?;
    let leaf = chain.first().ok_or_else(fail)?;
    let (_, parsed) = parse_x509_certificate(leaf.as_ref()).map_err(|_| fail())?;
    let san = parsed.subject_alternative_name().map_err(|_| fail())?.ok_or_else(fail)?;
    let matches = san.value.general_names.iter().any(|name| match name {
        GeneralName::DNSName(name) => {
            name.eq_ignore_ascii_case(domain)
                || name.strip_prefix("*.").is_some_and(|suffix| {
                    domain
                        .split_once('.')
                        .is_some_and(|(_, rest)| rest.eq_ignore_ascii_case(suffix))
                })
        }
        _ => false,
    });
    if !matches
        || parsed.validity().not_before.timestamp() > now()
        || parsed.validity().not_after.timestamp() <= now()
    {
        return Err(fail());
    }
    Ok(parsed.validity().not_after.timestamp())
}

pub async fn activate(
    repo: &SettingsRepo,
    request: &Request,
    cert: &str,
    key: &str,
    automatic: bool,
) -> Result<()> {
    let expires = expiry(cert, key, &request.domain)?.to_string();
    let retry_at = (now() + 86400).to_string();
    let renewal = if automatic {
        serde_json::to_string(request).map_err(Error::internal)?
    } else {
        String::new()
    };
    repo.set_many(&[
        ("tls_cert_pem", cert),
        ("tls_key_pem", key),
        ("tls_cert_expires_at", &expires),
        ("tls_hostnames", &request.domain),
        ("tls_mode", &request.mode),
        ("tls_renewal", &renewal),
        ("tls_retry_at", &retry_at),
        ("tls_pending", ""),
        ("tls_job_state", "ready"),
        ("tls_job_error", ""),
        ("tls_enabled", "true"),
    ])
    .await
}

/// Issue a server certificate from the local CA and store it. The caller holds the mutation
/// lock. A missing CA is created only when `create_ca` is set, so an unattended renewal can
/// never replace the root that client devices already trust.
pub async fn issue_local(repo: &SettingsRepo, hostnames: &[String], create_ca: bool) -> Result<()> {
    let names = clean_names(hostnames)?;
    let ca_key = match (repo.get("tls_ca_key_pem").await?, repo.get("tls_ca_cert_pem").await?) {
        (Some(key), Some(_)) => key,
        _ if create_ca => {
            let ca =
                tokio::task::spawn_blocking(generate_local_ca).await.map_err(Error::internal)??;
            repo.set_many(&[("tls_ca_cert_pem", &ca.cert_pem), ("tls_ca_key_pem", &ca.key_pem)])
                .await?;
            ca.key_pem
        }
        _ => {
            return Err(Error::Unavailable(
                "The local root CA is missing. Issue a new local certificate from Settings.".into(),
            ));
        }
    };
    let issue_names = names.clone();
    let pair = tokio::task::spawn_blocking(move || issue_server_cert(&ca_key, &issue_names))
        .await
        .map_err(Error::internal)??;
    let expires = pair.not_after.to_string();
    repo.set_many(&[
        ("tls_cert_pem", &pair.cert_pem),
        ("tls_key_pem", &pair.key_pem),
        ("tls_cert_expires_at", &expires),
        ("tls_hostnames", &names.join(",")),
        ("tls_mode", "local_ca"),
        ("tls_renewal", ""),
        ("tls_pending", ""),
        ("tls_job_state", "ready"),
        ("tls_job_error", ""),
    ])
    .await
}

/// Reissue the local certificate for the names it already covers.
async fn renew_local(repo: &SettingsRepo) -> Result<()> {
    let names: Vec<String> = repo
        .get("tls_hostnames")
        .await?
        .unwrap_or_default()
        .split(',')
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect();
    match issue_local(repo, &names, false).await {
        Ok(()) => tracing::info!("Local HTTPS certificate renewed"),
        Err(error) => {
            tracing::warn!(%error, "Local HTTPS certificate renewal failed");
            let retry = (now() + RETRY_AFTER).to_string();
            repo.set_many(&[("tls_job_error", &error.to_string()), ("tls_retry_at", &retry)])
                .await?;
        }
    }
    Ok(())
}

pub async fn enqueue(repo: &SettingsRepo, request: &Request) -> Result<()> {
    let _guard = lock_mutation().await?;
    if repo.get("tls_pending").await?.is_some_and(|v| !v.is_empty()) {
        return Err(Error::Unavailable(
            "A certificate request is already running. Check its status below.".into(),
        ));
    }
    let request = serde_json::to_string(request).map_err(Error::internal)?;
    repo.set_many(&[("tls_pending", &request), ("tls_job_state", "queued"), ("tls_job_error", "")])
        .await
}

/// One worker owns issuance. A durable pending request survives a server restart.
pub async fn tick(db: &Db) -> Result<()> {
    let _guard = match MUTATION.try_lock() {
        Ok(g) => g,
        Err(_) => return Ok(()),
    };
    let repo = SettingsRepo::new(db.clone());
    let pending = repo.get("tls_pending").await?.unwrap_or_default();
    const UNREADABLE: &str =
        "The saved certificate request could not be read. Request the certificate again.";
    let request = if !pending.is_empty() {
        match serde_json::from_str::<Request>(&pending) {
            Ok(request) => request,
            Err(_) => return failed(&repo, UNREADABLE).await,
        }
    } else {
        if repo.get("tls_enabled").await?.as_deref() != Some("true") {
            return Ok(());
        }
        let expires =
            repo.get("tls_cert_expires_at").await?.and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
        let retry =
            repo.get("tls_retry_at").await?.and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
        if expires > now() + RENEW_BEFORE || retry > now() {
            return Ok(());
        }
        let renewal = repo.get("tls_renewal").await?.unwrap_or_default();
        if renewal.is_empty() {
            if repo.get("tls_mode").await?.as_deref() == Some("local_ca") {
                return renew_local(&repo).await;
            }
            return Ok(());
        }
        match serde_json::from_str::<Request>(&renewal) {
            Ok(request) => request,
            Err(_) => return failed(&repo, UNREADABLE).await,
        }
    };
    repo.set("tls_job_state", "issuing").await?;
    let result = tokio::time::timeout(Duration::from_secs(300), issue(&repo, &request)).await;
    match result {
        Ok(Ok((cert, key))) => {
            if let Err(error) = activate(&repo, &request, &cert, &key, true).await {
                failed(&repo, &error.to_string()).await?;
            }
        }
        Ok(Err(error)) => failed(&repo, &error.to_string()).await?,
        Err(_) => {
            failed(&repo, "Certificate request timed out. Check network access and try again.")
                .await?
        }
    }
    Ok(())
}

async fn failed(repo: &SettingsRepo, message: &str) -> Result<()> {
    let retry = (now() + RETRY_AFTER).to_string();
    repo.set_many(&[
        ("tls_pending", ""),
        ("tls_job_state", "failed"),
        ("tls_job_error", message),
        ("tls_retry_at", &retry),
    ])
    .await
}

async fn issue(repo: &SettingsRepo, request: &Request) -> Result<(String, String)> {
    if request.mode == "tailscale" {
        let client = TailscaleClient::from_platform_default();
        let status = client.get_status().await.map_err(|error| {
            Error::Unavailable(format!(
                "Install and sign in to Tailscale on the MBL server. The MBL service must have permission to access Tailscale. ({error})"
            ))
        })?;
        if status.backend_state != "Running" {
            return Err(Error::Unavailable(
                "Open Tailscale on the server and sign in or reconnect.".into(),
            ));
        }
        let actual = status.magic_dns_name.unwrap_or_default().trim_end_matches('.').to_owned();
        if actual != request.domain {
            return Err(Error::Unavailable(
                "The Tailscale hostname changed. Fetch a new certificate from Settings.".into(),
            ));
        }
        let pair = client.fetch_cert_pair(&request.domain).await.map_err(|error| {
            Error::Unavailable(format!(
                "Enable MagicDNS and HTTPS Certificates in Tailscale DNS settings, and allow the MBL service to request certificates. ({error})"
            ))
        })?;
        return split_pair(&pair);
    }
    issue_acme(repo, request, LetsEncrypt::Production.url()).await
}

type Challenges = Arc<RwLock<HashMap<String, String>>>;

async fn challenge(
    State(values): State<Challenges>,
    Path(token): Path<String>,
) -> (StatusCode, String) {
    match values.read().ok().and_then(|v| v.get(&token).cloned()) {
        Some(value) => (StatusCode::OK, value),
        None => (StatusCode::NOT_FOUND, String::new()),
    }
}

async fn issue_acme(
    repo: &SettingsRepo,
    request: &Request,
    directory: &str,
) -> Result<(String, String)> {
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", CHALLENGE_PORT)).await
        .map_err(|_| Error::Unavailable("Cannot open certificate verification port 8081. Free this port on the MBL server and try again.".into()))?;
    let values: Challenges = Arc::default();
    let router = Router::new()
        .route("/.well-known/acme-challenge/{token}", get(challenge))
        .with_state(values.clone());
    // Dropping the issuance future also drops this server future and its listener.
    let issuance = async {
        let builder = Account::builder().map_err(Error::internal)?;
        let account = if let Some(saved) = repo.get("tls_acme_account").await? {
            builder
                .from_credentials(serde_json::from_str(&saved).map_err(Error::internal)?)
                .await
                .map_err(Error::internal)?
        } else {
            let contact = format!("mailto:{}", request.email);
            let contacts = if request.email.is_empty() { vec![] } else { vec![contact.as_str()] };
            let (account, credentials) = builder
                .create(
                    &NewAccount {
                        contact: &contacts,
                        terms_of_service_agreed: true,
                        only_return_existing: false,
                    },
                    directory.to_owned(),
                    None,
                )
                .await
                .map_err(Error::internal)?;
            repo.set(
                "tls_acme_account",
                &serde_json::to_string(&credentials).map_err(Error::internal)?,
            )
            .await?;
            account
        };
        let identifiers = [Identifier::Dns(request.domain.clone())];
        let mut order =
            account.new_order(&NewOrder::new(&identifiers)).await.map_err(Error::internal)?;
        {
            let mut authorizations = order.authorizations();
            while let Some(auth) = authorizations.next().await {
                let mut auth = auth.map_err(Error::internal)?;
                if auth.status == AuthorizationStatus::Valid {
                    continue;
                }
                let mut handle = auth.challenge(ChallengeType::Http01).ok_or_else(|| {
                    Error::Unavailable(
                        "Certificate authority did not offer HTTP verification".into(),
                    )
                })?;
                values
                    .write()
                    .map_err(|_| Error::Unavailable("Certificate verifier unavailable".into()))?
                    .insert(handle.token.clone(), handle.key_authorization().as_str().to_owned());
                handle.set_ready().await.map_err(Error::internal)?;
            }
        }
        let ready = match order.poll_ready(&RetryPolicy::default()).await {
            Ok(status) => status,
            Err(error) => {
                tracing::warn!(%error, domain = %request.domain, "ACME order did not become ready");
                return Err(validation_error(Some(&error.to_string())));
            }
        };
        if ready != OrderStatus::Ready {
            // The order only says "invalid"; the reason is on the failed challenge.
            let mut reason = None;
            let mut authorizations = order.authorizations();
            while let Some(Ok(auth)) = authorizations.next().await {
                reason = auth
                    .challenges
                    .iter()
                    .find_map(|challenge| challenge.error.as_ref())
                    .map(|problem| problem.to_string());
                if reason.is_some() {
                    break;
                }
            }
            tracing::warn!(
                domain = %request.domain,
                status = ?ready,
                reason = reason.as_deref().unwrap_or("not reported"),
                "ACME hostname verification failed"
            );
            return Err(validation_error(reason.as_deref()));
        }
        let key = order.finalize().await.map_err(Error::internal)?;
        let cert =
            order.poll_certificate(&RetryPolicy::default()).await.map_err(Error::internal)?;
        Ok((cert, key))
    };
    tokio::select! {
        result = issuance => result,
        result = axum::serve(listener, router).into_future() => {
            result.map_err(Error::internal)?;
            Err(Error::Unavailable("Certificate verification listener stopped".into()))
        }
    }
}

fn validation_error(reason: Option<&str>) -> Error {
    let mut message = String::from(
        "Let's Encrypt could not verify your hostname. Point DNS at your public IP, forward external TCP port 80 to this MBL server on port 8081, allow port 8081 in the server firewall, and check for an incorrect IPv6/AAAA record or CGNAT. Keep forwarding enabled for renewal.",
    );
    if let Some(reason) = reason {
        message.push_str(" Reported cause: ");
        message.extend(reason.chars().filter(|c| !c.is_control()).take(400));
    }
    Error::Unavailable(message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mbl_remote::tls::{generate_local_ca, issue_server_cert};

    #[test]
    fn domain_name_validation() {
        assert_eq!(domain_name("  MBL.TPLINKDNS.COM.  ").unwrap(), "mbl.tplinkdns.com");
        assert_eq!(domain_name("sub.domain.co.uk").unwrap(), "sub.domain.co.uk");
        assert!(domain_name("localhost").is_err());
        assert!(domain_name("192.168.1.1").is_err());
        assert!(domain_name("server.local").is_err());
        assert!(domain_name("-test.com").is_err());
        assert!(domain_name("test-.com").is_err());
        assert!(domain_name("").is_err());
        assert!(domain_name("https://example.com").is_err());
    }

    /// Validate a served chain the way a TLS client does: signature, validity and host name.
    fn client_accepts(ca_pem: &str, cert_pem: &str, host: &str) -> bool {
        use rustls::client::danger::ServerCertVerifier;
        use rustls::pki_types::{ServerName, UnixTime};

        let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
        let mut roots = rustls::RootCertStore::empty();
        for cert in rustls_pemfile::certs(&mut ca_pem.as_bytes()) {
            roots.add(cert.unwrap()).unwrap();
        }
        let verifier =
            rustls::client::WebPkiServerVerifier::builder_with_provider(Arc::new(roots), provider)
                .build()
                .unwrap();
        let chain: Vec<_> =
            rustls_pemfile::certs(&mut cert_pem.as_bytes()).map(|cert| cert.unwrap()).collect();
        let name = ServerName::try_from(host.to_owned()).unwrap();
        verifier.verify_server_cert(&chain[0], &chain[1..], &name, &[], UnixTime::now()).is_ok()
    }

    #[test]
    fn issued_certificates_chain_to_the_installed_root() {
        let ca = generate_local_ca().expect("ca");
        let names = ["mbl.home.lan".to_owned(), "*.mbl.lan".to_owned(), "192.168.1.10".to_owned()];
        // The issuer is rebuilt from the stored key on every call; both must validate.
        for _ in 0..2 {
            let server = issue_server_cert(&ca.key_pem, &names).expect("server cert");
            for host in ["mbl.home.lan", "tv.mbl.lan", "192.168.1.10"] {
                assert!(client_accepts(&ca.cert_pem, &server.cert_pem, host), "{host}");
            }
            for host in ["other.home.lan", "a.b.mbl.lan", "192.168.1.11"] {
                assert!(!client_accepts(&ca.cert_pem, &server.cert_pem, host), "{host}");
            }
            let stranger = generate_local_ca().expect("other ca");
            assert!(!client_accepts(&stranger.cert_pem, &server.cert_pem, "mbl.home.lan"));
        }
    }

    #[test]
    fn local_ca_cannot_create_another_authority() {
        use x509_parser::prelude::*;
        let ca = generate_local_ca().expect("ca");
        let der = rustls_pemfile::certs(&mut ca.cert_pem.as_bytes()).next().unwrap().unwrap();
        let (_, parsed) = parse_x509_certificate(der.as_ref()).unwrap();
        let constraints = parsed.basic_constraints().unwrap().expect("basic constraints").value;
        assert!(constraints.ca);
        assert_eq!(constraints.path_len_constraint, Some(0));
    }

    /// `tick` skips its work while another task holds the process-wide mutation lock, so the
    /// tests that drive it must not overlap.
    static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    async fn test_repo() -> (tempfile::TempDir, SettingsRepo) {
        let dir = tempfile::tempdir().unwrap();
        let path = camino::Utf8PathBuf::from_path_buf(dir.path().join("mbl.db")).unwrap();
        let db = Db::connect(&path, &mbl_db::DbOptions::default()).await.unwrap();
        (dir, SettingsRepo::new(db))
    }

    #[tokio::test]
    async fn local_certificate_is_renewed_before_expiry_without_replacing_the_root() {
        let _serial = SERIAL.lock().await;
        let (_dir, repo) = test_repo().await;
        let db = Db::clone(repo.db());
        {
            let _guard = lock_mutation().await.unwrap();
            assert!(issue_local(&repo, &["mbl.home.lan".into()], false).await.is_err());
            issue_local(&repo, &["mbl.home.lan".into(), "10.0.0.5".into()], true).await.unwrap();
        }
        repo.set("tls_enabled", "true").await.unwrap();
        let root = repo.get("tls_ca_cert_pem").await.unwrap().unwrap();
        let first = repo.get("tls_cert_pem").await.unwrap().unwrap();

        // Plenty of validity left: nothing happens.
        tick(&db).await.unwrap();
        assert_eq!(repo.get("tls_cert_pem").await.unwrap().unwrap(), first);

        // Inside the renewal window: a fresh certificate for the same names and the same root.
        repo.set("tls_cert_expires_at", &(now() + 86400).to_string()).await.unwrap();
        tick(&db).await.unwrap();
        let renewed = repo.get("tls_cert_pem").await.unwrap().unwrap();
        assert_ne!(renewed, first);
        assert_eq!(repo.get("tls_ca_cert_pem").await.unwrap().unwrap(), root);
        assert_eq!(repo.get("tls_hostnames").await.unwrap().unwrap(), "mbl.home.lan,10.0.0.5");
        let expires: i64 = repo.get("tls_cert_expires_at").await.unwrap().unwrap().parse().unwrap();
        assert!(expires > now() + 300 * 86400);
        assert!(client_accepts(&root, &renewed, "10.0.0.5"));

        // A lost root is reported and retried later instead of being silently replaced.
        repo.set("tls_cert_expires_at", "0").await.unwrap();
        repo.delete("tls_ca_key_pem").await.unwrap();
        tick(&db).await.unwrap();
        assert_eq!(repo.get("tls_cert_pem").await.unwrap().unwrap(), renewed);
        assert!(repo.get("tls_job_error").await.unwrap().unwrap().contains("root CA is missing"));
        let retry: i64 = repo.get("tls_retry_at").await.unwrap().unwrap().parse().unwrap();
        assert!(retry > now());
    }

    #[tokio::test]
    async fn unreadable_saved_request_is_cleared_instead_of_failing_forever() {
        let _serial = SERIAL.lock().await;
        let (_dir, repo) = test_repo().await;
        let db = Db::clone(repo.db());
        repo.set("tls_pending", "{not json").await.unwrap();
        tick(&db).await.unwrap();
        assert_eq!(repo.get("tls_pending").await.unwrap().unwrap(), "");
        assert_eq!(repo.get("tls_job_state").await.unwrap().unwrap(), "failed");
        assert!(repo.get("tls_job_error").await.unwrap().unwrap().contains("could not be read"));
    }

    #[test]
    fn certificate_expiry_and_matching() {
        let ca = generate_local_ca().expect("ca");
        let server =
            issue_server_cert(&ca.key_pem, &["mbl.example.com".into()]).expect("server cert");

        let exp = expiry(&server.cert_pem, &server.key_pem, "mbl.example.com").expect("expiry");
        assert!(exp > now());

        assert!(expiry(&server.cert_pem, &server.key_pem, "other.example.com").is_err());
        assert!(expiry(&server.cert_pem, &ca.key_pem, "mbl.example.com").is_err());
    }
}
