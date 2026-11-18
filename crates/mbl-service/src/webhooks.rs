//! Webhook notifications service (M7.11).

use hmac::{Hmac, KeyInit, Mac};
use mbl_core::{Actor, Error, Result, WebhookId};
use mbl_db::Db;
use mbl_db::repo::m7;
pub use mbl_db::repo::m7::WebhookRow;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::sync::Arc;
use std::time::Duration;
use utoipa::ToSchema;

type HmacSha256 = Hmac<Sha256>;

/// Events a webhook can subscribe to; `*` means all of them.
pub const WEBHOOK_EVENTS: &[&str] = &[
    "*",
    "playback.started",
    "playback.stopped",
    "media.added",
    "library.scan_completed",
    "library.unavailable",
];

fn mask_secret(mut row: WebhookRow) -> WebhookRow {
    if row.secret.as_deref().is_some_and(|s| !s.is_empty()) {
        row.secret = Some("********".into());
    }
    row
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateWebhookRequest {
    pub url: String,
    pub name: String,
    pub events: Vec<String>,
    #[serde(default)]
    pub secret: Option<String>,
}

#[derive(Debug, Clone)]
pub struct WebhookService {
    db: Arc<Db>,
    http_client: reqwest::Client,
}

impl WebhookService {
    pub fn new(db: Arc<Db>) -> Self {
        let http_client =
            reqwest::Client::builder().timeout(Duration::from_secs(10)).build().unwrap_or_default();
        Self { db, http_client }
    }

    pub async fn create_webhook(
        &self,
        actor: &Actor,
        req: CreateWebhookRequest,
    ) -> Result<WebhookRow> {
        actor.require_admin()?;
        if req.name.trim().is_empty() || req.events.is_empty() {
            return Err(Error::Validation("A webhook needs a name and at least one event".into()));
        }
        if let Some(unknown) = req.events.iter().find(|e| !WEBHOOK_EVENTS.contains(&e.as_str())) {
            return Err(Error::Validation(format!("Unknown webhook event: {unknown}")));
        }
        if reqwest::Url::parse(&req.url).is_err() {
            return Err(Error::Validation("Webhook URL is not a valid URL".into()));
        }
        if !req.url.starts_with("http://") && !req.url.starts_with("https://") {
            return Err(Error::Validation(
                "Webhook URL must use http:// or https:// protocol".into(),
            ));
        }

        let id =
            m7::create_webhook(&self.db, &req.url, &req.name, &req.events, req.secret.as_deref())
                .await?;

        let list = m7::list_webhooks(&self.db).await?;
        list.into_iter().find(|w| w.id == id).map(mask_secret).ok_or(Error::NotFound)
    }

    /// Webhooks with the signing secret replaced by `********`; it is only shown when created.
    pub async fn list_webhooks(&self, actor: &Actor) -> Result<Vec<WebhookRow>> {
        actor.require_admin()?;
        Ok(m7::list_webhooks(&self.db).await?.into_iter().map(mask_secret).collect())
    }

    pub async fn delete_webhook(&self, actor: &Actor, id: WebhookId) -> Result<()> {
        actor.require_admin()?;
        if !m7::delete_webhook(&self.db, id).await? {
            return Err(Error::NotFound);
        }
        Ok(())
    }

    pub async fn set_enabled(&self, actor: &Actor, id: WebhookId, enabled: bool) -> Result<()> {
        actor.require_admin()?;
        if !m7::set_webhook_enabled(&self.db, id, enabled).await? {
            return Err(Error::NotFound);
        }
        Ok(())
    }

    /// Send `payload` to every enabled webhook subscribed to `event_name` (or `*`).
    /// Delivery runs in the background; failures are logged and retried once after 10 s.
    pub async fn dispatch_event(&self, event_name: &str, payload: &serde_json::Value) {
        let Ok(hooks) = m7::list_webhooks(&self.db).await else {
            return;
        };
        for hook in hooks {
            if !hook.enabled || !hook.events.iter().any(|e| e == "*" || e == event_name) {
                continue;
            }
            let client = self.http_client.clone();
            let event = event_name.to_owned();
            let body = payload.to_string();
            tokio::spawn(async move {
                for attempt in 0..2 {
                    if attempt > 0 {
                        tokio::time::sleep(Duration::from_secs(10)).await;
                    }
                    match send(&client, &hook, &event, &body).await {
                        Ok(_) => return,
                        Err(error) => tracing::warn!(
                            webhook = %hook.name, event = %event, attempt = attempt + 1, %error,
                            "webhook delivery failed"
                        ),
                    }
                }
            });
        }
    }

    /// Send a `test` event to one webhook now and report the receiver's HTTP status.
    pub async fn test_webhook(&self, actor: &Actor, id: WebhookId) -> Result<WebhookTestResult> {
        actor.require_admin()?;
        let hook = m7::list_webhooks(&self.db)
            .await?
            .into_iter()
            .find(|w| w.id == id)
            .ok_or(Error::NotFound)?;
        let body = serde_json::json!({
            "event": "test",
            "timestamp": mbl_core::time::now_ms(),
            "message": "Test notification from Media Box Lounge",
        })
        .to_string();
        Ok(match send(&self.http_client, &hook, "test", &body).await {
            Ok(status) => WebhookTestResult { ok: true, status: Some(status), error: None },
            Err(error) => {
                WebhookTestResult { ok: false, status: error.status, error: Some(error.message) }
            }
        })
    }
}

/// Outcome of `POST /api/v1/webhooks/{id}/test`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WebhookTestResult {
    pub ok: bool,
    /// HTTP status returned by the receiver, when it answered.
    pub status: Option<u16>,
    pub error: Option<String>,
}

#[derive(Debug)]
struct SendError {
    status: Option<u16>,
    message: String,
}

impl std::fmt::Display for SendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

/// POST one event, signed with `X-MBL-Signature: sha256=<hex HMAC of the body>` when the
/// webhook has a secret.
async fn send(
    client: &reqwest::Client,
    hook: &WebhookRow,
    event: &str,
    body: &str,
) -> std::result::Result<u16, SendError> {
    let mut req = client
        .post(&hook.url)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header("X-MBL-Event", event)
        .body(body.to_owned());
    if let Some(secret) = hook.secret.as_deref().filter(|s| !s.is_empty())
        && let Ok(mut mac) = HmacSha256::new_from_slice(secret.as_bytes())
    {
        mac.update(body.as_bytes());
        let signature: String =
            mac.finalize().into_bytes().iter().map(|b| format!("{b:02x}")).collect();
        req = req.header("X-MBL-Signature", format!("sha256={signature}"));
    }
    let res = req
        .send()
        .await
        .map_err(|e| SendError { status: None, message: format!("could not connect: {e}") })?;
    let status = res.status();
    if status.is_success() {
        Ok(status.as_u16())
    } else {
        Err(SendError {
            status: Some(status.as_u16()),
            message: format!("receiver answered {status}"),
        })
    }
}
