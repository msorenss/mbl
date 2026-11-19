//! Settings for the built-in assistant: which language model provider it uses.
//!
//! The provider is reached over HTTP with an OpenAI-compatible or Anthropic protocol (see
//! `mbl-agent`). Only admins can read or change the settings; the API key is stored like the
//! other service keys and never returned.

use mbl_agent::{ConnectionReport, Provider, ProviderConfig, ProviderKind};
use mbl_core::{Actor, Error, Result};
use mbl_db::Db;
use mbl_db::repo::settings::SettingsRepo;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

const ENABLED: &str = "assistant_enabled";
const PROVIDER: &str = "assistant_provider";
const BASE_URL: &str = "assistant_base_url";
const API_KEY: &str = "assistant_api_key";
const MODEL: &str = "assistant_model";
const EFFORT: &str = "assistant_effort";
const EFFORTS: &[&str] = &["low", "medium", "high"];

/// Stored assistant settings. The API key is never returned.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AssistantSettings {
    pub enabled: bool,
    pub provider: ProviderKind,
    pub base_url: String,
    pub has_api_key: bool,
    pub model: String,
    /// `low`, `medium` or `high`; empty means the provider's default.
    pub effort: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAssistantSettings {
    pub enabled: bool,
    pub provider: ProviderKind,
    pub base_url: String,
    /// New key; omit or leave empty to keep the stored one.
    #[serde(default)]
    pub api_key: Option<String>,
    /// Remove the stored key (for local servers that need none).
    #[serde(default)]
    pub clear_api_key: bool,
    pub model: String,
    #[serde(default)]
    pub effort: Option<String>,
}

/// Connection details to try before saving. Without `apiKey` the stored key is used.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AssistantConnection {
    pub provider: ProviderKind,
    pub base_url: String,
    #[serde(default)]
    pub api_key: Option<String>,
    /// Needed for the connection test; ignored when listing models.
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub effort: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AssistantService {
    db: Db,
}

impl AssistantService {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    fn settings(&self) -> SettingsRepo {
        SettingsRepo::new(self.db.clone())
    }

    async fn stored(&self, key: &str) -> Result<Option<String>> {
        Ok(self.settings().get(key).await?.filter(|v| !v.is_empty()))
    }

    pub async fn get_settings(&self, actor: &Actor) -> Result<AssistantSettings> {
        actor.require_admin()?;
        self.read().await
    }

    async fn read(&self) -> Result<AssistantSettings> {
        Ok(AssistantSettings {
            enabled: self.stored(ENABLED).await?.as_deref() == Some("true"),
            provider: match self.stored(PROVIDER).await?.as_deref() {
                Some("anthropic") => ProviderKind::Anthropic,
                _ => ProviderKind::OpenaiCompatible,
            },
            base_url: self.stored(BASE_URL).await?.unwrap_or_default(),
            has_api_key: self.stored(API_KEY).await?.is_some(),
            model: self.stored(MODEL).await?.unwrap_or_default(),
            effort: self.stored(EFFORT).await?,
        })
    }

    pub async fn update_settings(
        &self,
        actor: &Actor,
        update: UpdateAssistantSettings,
    ) -> Result<AssistantSettings> {
        actor.require_admin()?;
        let effort = clean_effort(update.effort)?;
        let base_url = if update.base_url.trim().is_empty() {
            String::new()
        } else {
            mbl_agent::normalize_base_url(update.provider, &update.base_url)?
        };
        let model = update.model.trim().to_owned();
        if update.enabled && (base_url.is_empty() || model.is_empty()) {
            return Err(Error::Validation(
                "The assistant needs an address and a model before it can be turned on".into(),
            ));
        }
        let new_key = update.api_key.map(|k| k.trim().to_owned()).filter(|k| !k.is_empty());
        let mut entries = vec![
            (ENABLED, if update.enabled { "true".to_owned() } else { "false".to_owned() }),
            (PROVIDER, provider_name(update.provider).to_owned()),
            (BASE_URL, base_url),
            (MODEL, model),
            (EFFORT, effort.unwrap_or_default()),
        ];
        if let Some(key) = new_key {
            entries.push((API_KEY, key));
        } else if update.clear_api_key {
            entries.push((API_KEY, String::new()));
        }
        let borrowed: Vec<(&str, &str)> = entries.iter().map(|(k, v)| (*k, v.as_str())).collect();
        self.settings().set_many(&borrowed).await?;
        tracing::info!(
            enabled = update.enabled,
            provider = provider_name(update.provider),
            "assistant settings saved"
        );
        self.read().await
    }

    async fn provider_for(&self, input: AssistantConnection) -> Result<Provider> {
        let key = match input.api_key.map(|k| k.trim().to_owned()).filter(|k| !k.is_empty()) {
            Some(key) => Some(key),
            None => self.stored(API_KEY).await?,
        };
        Provider::new(ProviderConfig {
            kind: input.provider,
            base_url: input.base_url,
            api_key: key,
            // Listing models needs no model; any name passes validation.
            model: if input.model.trim().is_empty() { "-".into() } else { input.model },
            effort: clean_effort(input.effort)?,
        })
    }

    /// Asks the model a short question and checks that it can call tools.
    pub async fn test_connection(
        &self,
        actor: &Actor,
        input: AssistantConnection,
    ) -> Result<ConnectionReport> {
        actor.require_admin()?;
        if input.model.trim().is_empty() {
            return Err(Error::Validation("Choose a model".into()));
        }
        Ok(self.provider_for(input).await?.test().await)
    }

    /// Models offered by the provider.
    pub async fn list_models(
        &self,
        actor: &Actor,
        input: AssistantConnection,
    ) -> Result<Vec<String>> {
        actor.require_admin()?;
        self.provider_for(input).await?.list_models().await
    }

    /// Whether the assistant is turned on; cheap enough for frequent polling.
    pub async fn is_enabled(&self) -> bool {
        matches!(self.stored(ENABLED).await, Ok(Some(v)) if v == "true")
    }

    /// The configured provider when the assistant is turned on.
    pub async fn provider(&self) -> Result<Option<Provider>> {
        let settings = self.read().await?;
        if !settings.enabled {
            return Ok(None);
        }
        Ok(Some(Provider::new(ProviderConfig {
            kind: settings.provider,
            base_url: settings.base_url,
            api_key: self.stored(API_KEY).await?,
            model: settings.model,
            effort: settings.effort,
        })?))
    }
}

fn provider_name(kind: ProviderKind) -> &'static str {
    match kind {
        ProviderKind::OpenaiCompatible => "openai_compatible",
        ProviderKind::Anthropic => "anthropic",
    }
}

fn clean_effort(effort: Option<String>) -> Result<Option<String>> {
    match effort.map(|e| e.trim().to_lowercase()).filter(|e| !e.is_empty()) {
        None => Ok(None),
        Some(e) if EFFORTS.contains(&e.as_str()) => Ok(Some(e)),
        Some(e) => Err(Error::Validation(format!("Unknown effort '{e}'; use low, medium or high"))),
    }
}
