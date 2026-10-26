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

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateWebhookRequest {
    pub url: String,
    pub name: String,
    pub events: Vec<String>,
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
        let id =
            m7::create_webhook(&self.db, &req.url, &req.name, &req.events, req.secret.as_deref())
                .await?;

        let list = m7::list_webhooks(&self.db).await?;
        list.into_iter().find(|w| w.id == id).ok_or(Error::NotFound)
    }

    pub async fn list_webhooks(&self, actor: &Actor) -> Result<Vec<WebhookRow>> {
        actor.require_admin()?;
        m7::list_webhooks(&self.db).await
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

    /// Dispatches an event payload to all matching enabled webhooks.
    pub async fn dispatch_event(&self, event_name: &str, payload_json: &str) {
        let Ok(hooks) = m7::list_webhooks(&self.db).await else {
            return;
        };

        for hook in hooks {
            if !hook.enabled {
                continue;
            }
            if !hook.events.iter().any(|e| e == "*" || e == event_name) {
                continue;
            }

            let mut req = self
                .http_client
                .post(&hook.url)
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .header("X-MBL-Event", event_name)
                .body(payload_json.to_owned());

            if let Some(ref sec) = hook.secret
                && let Ok(mut mac) = HmacSha256::new_from_slice(sec.as_bytes())
            {
                mac.update(payload_json.as_bytes());
                let signature: String =
                    mac.finalize().into_bytes().iter().map(|b| format!("{b:02x}")).collect();
                req = req.header("X-MBL-Signature", format!("sha256={signature}"));
            }

            tokio::spawn(async move {
                let _ = req.send().await;
            });
        }
    }
}
