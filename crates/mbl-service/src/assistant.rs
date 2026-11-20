//! Settings for the built-in assistant: which language model provider it uses.
//!
//! The provider is reached over HTTP with an OpenAI-compatible or Anthropic protocol (see
//! `mbl-agent`). Only admins can read or change the settings; the API key is stored like the
//! other service keys and never returned.

use std::sync::{Arc, RwLock};

use mbl_agent::{ConnectionReport, Message, Provider, ProviderConfig, ProviderKind, TextSink};
use mbl_auth::tokens::{Scope, ScopeSet};
use mbl_core::time::now_ms;
use mbl_core::{Actor, Error, Result};
use mbl_db::Db;
use mbl_db::repo::assistant::{AssistantRepo, Conversation, ConversationMessage, ItemMention};
use mbl_db::repo::settings::SettingsRepo;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

const ENABLED: &str = "assistant_enabled";
const PROVIDER: &str = "assistant_provider";
const BASE_URL: &str = "assistant_base_url";
const API_KEY: &str = "assistant_api_key";
const MODEL: &str = "assistant_model";
const EFFORT: &str = "assistant_effort";
const COMMAND: &str = "assistant_command";
const SCOPES_ADMIN: &str = "assistant_scopes_admin";
const SCOPES_USER: &str = "assistant_scopes_user";
/// What the assistant may do until an admin chooses otherwise.
const DEFAULT_SCOPES: &str = "read,playback";
/// Longest accepted agent command.
const COMMAND_LIMIT: usize = 1000;
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
    /// The agent program run for the `command` provider.
    #[serde(default)]
    pub command: String,
    /// What the assistant may do for administrators (token scopes; `read` is always included).
    #[serde(default)]
    pub admin_scopes: Vec<String>,
    /// What it may do for other users; never `admin`, `users` or `remote`.
    #[serde(default)]
    pub user_scopes: Vec<String>,
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
    /// For the `command` provider, e.g. `claude -p --mcp-config {mcp_config}`.
    #[serde(default)]
    pub command: Option<String>,
    /// Unchanged when left out.
    #[serde(default)]
    pub admin_scopes: Option<Vec<String>>,
    /// Unchanged when left out.
    #[serde(default)]
    pub user_scopes: Option<Vec<String>>,
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
    /// For the `command` provider.
    #[serde(default)]
    pub command: Option<String>,
}

/// How the assistant answers.
#[derive(Debug, Clone)]
pub enum AssistantEngine {
    /// A language model over HTTP; MBL runs the tool loop.
    Model(Provider),
    /// An agent program run as a command; it uses MBL's MCP server itself.
    Command(String),
}

/// Messages of a conversation given to the model.
const CHAT_HISTORY: i64 = 20;
const CHAT_MESSAGE_LIMIT: usize = 2000;

/// What answers chat messages: the agent loop with MBL's tools. It lives in `mbl-mcp`, which
/// depends on this crate, so the server plugs it in at start.
#[async_trait::async_trait]
pub trait AssistantBackend: Send + Sync {
    /// Answers the conversation (ending with the person's message) as `actor`.
    async fn answer(
        &self,
        actor: &Actor,
        conversation: Vec<Message>,
        on_text: Option<TextSink<'_>>,
    ) -> Result<AssistantAnswer>;

    /// Runs an agent command once with a short question, as `actor`.
    async fn test_command(&self, actor: &Actor, command: &str) -> ConnectionReport;
}

#[derive(Debug, Clone, Default)]
pub struct AssistantAnswer {
    pub text: String,
    pub refused: bool,
    /// Tools used, in order.
    pub tools: Vec<String>,
    /// Library titles the answer mentions.
    pub items: Vec<ItemMention>,
    /// A title to start in the person's browser.
    pub play: Option<ItemMention>,
}

/// Whether the assistant chat can be used by the person asking.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AssistantStatus {
    pub available: bool,
}

/// A conversation with its messages, oldest first.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConversationDetail {
    pub conversation: Conversation,
    pub messages: Vec<ConversationMessage>,
}

#[derive(Clone)]
pub struct AssistantService {
    db: Db,
    backend: Arc<RwLock<Option<Arc<dyn AssistantBackend>>>>,
}

impl std::fmt::Debug for AssistantService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AssistantService").finish_non_exhaustive()
    }
}

impl AssistantService {
    pub fn new(db: Db) -> Self {
        Self { db, backend: Arc::new(RwLock::new(None)) }
    }

    /// Plugs in the agent loop; shared by every clone of the service.
    pub fn set_backend(&self, backend: Arc<dyn AssistantBackend>) {
        if let Ok(mut slot) = self.backend.write() {
            *slot = Some(backend);
        }
    }

    pub fn backend(&self) -> Option<Arc<dyn AssistantBackend>> {
        self.backend.read().ok().and_then(|slot| slot.clone())
    }

    fn repo(&self) -> AssistantRepo {
        AssistantRepo::new(self.db.clone())
    }

    pub async fn status(&self, actor: &Actor) -> AssistantStatus {
        AssistantStatus {
            available: !actor.is_managed && self.backend().is_some() && self.is_enabled().await,
        }
    }

    pub async fn conversations(&self, actor: &Actor) -> Result<Vec<Conversation>> {
        self.repo().list(actor.user_id).await
    }

    pub async fn conversation(&self, actor: &Actor, id: i64) -> Result<ConversationDetail> {
        let repo = self.repo();
        let conversation = repo.get(actor.user_id, id).await?.ok_or(Error::NotFound)?;
        let messages = repo.messages(actor.user_id, id, 500).await?;
        Ok(ConversationDetail { conversation, messages })
    }

    pub async fn delete_conversation(&self, actor: &Actor, id: i64) -> Result<()> {
        if self.repo().delete(actor.user_id, id).await? { Ok(()) } else { Err(Error::NotFound) }
    }

    /// Stores the person's message (starting a conversation when `conversation` is `None`) and
    /// returns the conversation with the turns to send to the model.
    pub async fn begin_chat(
        &self,
        actor: &Actor,
        conversation: Option<i64>,
        text: &str,
    ) -> Result<(Conversation, Vec<Message>)> {
        if !self.status(actor).await.available {
            return Err(if actor.is_managed {
                Error::Forbidden
            } else {
                Error::Unavailable("the assistant is turned off".into())
            });
        }
        let text = text.trim();
        if text.is_empty() || text.chars().count() > CHAT_MESSAGE_LIMIT {
            return Err(Error::Validation(format!(
                "message must contain 1–{CHAT_MESSAGE_LIMIT} characters"
            )));
        }
        let repo = self.repo();
        let now = now_ms();
        let conversation = match conversation {
            Some(id) => repo.get(actor.user_id, id).await?.ok_or(Error::NotFound)?,
            None => repo.create(actor.user_id, &title_from(text), now).await?,
        };
        repo.add_message(actor.user_id, conversation.id, "user", text, &[], now)
            .await?
            .ok_or(Error::NotFound)?;
        let turns = repo
            .messages(actor.user_id, conversation.id, CHAT_HISTORY)
            .await?
            .into_iter()
            .map(|m| {
                if m.role == "assistant" {
                    Message::assistant(m.content)
                } else {
                    Message::user(m.content)
                }
            })
            .skip_while(|m| m.role == mbl_agent::Role::Assistant)
            .collect();
        Ok((conversation, turns))
    }

    /// Stores the assistant's answer.
    pub async fn finish_chat(
        &self,
        actor: &Actor,
        conversation: i64,
        text: &str,
        items: &[ItemMention],
    ) -> Result<ConversationMessage> {
        self.repo()
            .add_message(actor.user_id, conversation, "assistant", text, items, now_ms())
            .await?
            .ok_or(Error::NotFound)
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
                Some("command") => ProviderKind::Command,
                _ => ProviderKind::OpenaiCompatible,
            },
            base_url: self.stored(BASE_URL).await?.unwrap_or_default(),
            has_api_key: self.stored(API_KEY).await?.is_some(),
            model: self.stored(MODEL).await?.unwrap_or_default(),
            effort: self.stored(EFFORT).await?,
            command: self.stored(COMMAND).await?.unwrap_or_default(),
            admin_scopes: self.stored_scopes(SCOPES_ADMIN).await?.to_strings(),
            user_scopes: self.stored_scopes(SCOPES_USER).await?.to_strings(),
        })
    }

    async fn stored_scopes(&self, key: &str) -> Result<ScopeSet> {
        let raw = self.stored(key).await?.unwrap_or_else(|| DEFAULT_SCOPES.to_owned());
        let names: Vec<String> = raw.split(',').map(str::to_owned).collect();
        Ok(with_read(ScopeSet::parse_list(&names).unwrap_or_default()))
    }

    /// What the assistant may do for `actor`: the role's scopes, never more than the person's
    /// own rights.
    pub async fn scopes_for(&self, actor: &Actor) -> Result<ScopeSet> {
        let key = if actor.is_admin { SCOPES_ADMIN } else { SCOPES_USER };
        Ok(self.stored_scopes(key).await?.capped_for_user(actor.is_admin))
    }

    pub async fn update_settings(
        &self,
        actor: &Actor,
        update: UpdateAssistantSettings,
    ) -> Result<AssistantSettings> {
        actor.require_admin()?;
        let effort = clean_effort(update.effort)?;
        let command = clean_command(update.command.as_deref())?;
        let is_command = update.provider == ProviderKind::Command;
        let base_url = if update.base_url.trim().is_empty() || is_command {
            update.base_url.trim().to_owned()
        } else {
            mbl_agent::normalize_base_url(update.provider, &update.base_url)?
        };
        let model = update.model.trim().to_owned();
        if update.enabled && is_command && command.is_empty() {
            return Err(Error::Validation(
                "Enter the command before turning the assistant on".into(),
            ));
        }
        if update.enabled && !is_command && (base_url.is_empty() || model.is_empty()) {
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
            (COMMAND, command),
        ];
        if let Some(scopes) = update.admin_scopes {
            entries.push((SCOPES_ADMIN, clean_scopes(&scopes, true)?));
        }
        if let Some(scopes) = update.user_scopes {
            entries.push((SCOPES_USER, clean_scopes(&scopes, false)?));
        }
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
        if input.provider == ProviderKind::Command {
            let command = clean_command(input.command.as_deref())?;
            if command.is_empty() {
                return Err(Error::Validation("Enter the command".into()));
            }
            let backend = self
                .backend()
                .ok_or_else(|| Error::Unavailable("the assistant is not ready".into()))?;
            return Ok(backend.test_command(actor, &command).await);
        }
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

    /// How the assistant answers, when it is turned on.
    pub async fn engine(&self) -> Result<Option<AssistantEngine>> {
        let settings = self.read().await?;
        if !settings.enabled {
            return Ok(None);
        }
        if settings.provider == ProviderKind::Command {
            return Ok(Some(AssistantEngine::Command(settings.command)));
        }
        Ok(Some(AssistantEngine::Model(Provider::new(ProviderConfig {
            kind: settings.provider,
            base_url: settings.base_url,
            api_key: self.stored(API_KEY).await?,
            model: settings.model,
            effort: settings.effort,
        })?)))
    }

    /// The configured language model when the assistant is on and uses one.
    pub async fn provider(&self) -> Result<Option<Provider>> {
        Ok(match self.engine().await? {
            Some(AssistantEngine::Model(provider)) => Some(provider),
            _ => None,
        })
    }
}

/// The first words of the opening message, as the conversation's name.
fn title_from(text: &str) -> String {
    let line = text.lines().next().unwrap_or(text).trim();
    if line.chars().count() <= 60 {
        return line.to_owned();
    }
    let cut: String = line.chars().take(60).collect();
    let cut = cut.rsplit_once(' ').map_or(cut.as_str(), |(head, _)| head).to_owned();
    format!("{cut}…")
}

fn provider_name(kind: ProviderKind) -> &'static str {
    match kind {
        ProviderKind::OpenaiCompatible => "openai_compatible",
        ProviderKind::Anthropic => "anthropic",
        ProviderKind::Command => "command",
    }
}

/// Scopes always include `read`.
fn with_read(mut scopes: ScopeSet) -> ScopeSet {
    scopes.0.insert(Scope::Read);
    scopes
}

/// Validated scope names as stored, `read` included.
fn clean_scopes(names: &[String], for_admins: bool) -> Result<String> {
    let mut names = names.to_vec();
    names.push("read".into());
    let scopes = with_read(ScopeSet::parse_list(&names)?);
    if !for_admins && scopes.0.iter().any(|s| s.is_admin_only()) {
        return Err(Error::Validation(
            "Other users cannot give the assistant admin, users or remote access".into(),
        ));
    }
    Ok(scopes.to_strings().join(","))
}

fn clean_command(command: Option<&str>) -> Result<String> {
    let command = command.unwrap_or_default().trim();
    if command.chars().count() > COMMAND_LIMIT || command.contains(['\n', '\r', '\0']) {
        return Err(Error::Validation(
            "The command must be one line of at most 1000 characters".into(),
        ));
    }
    Ok(command.to_owned())
}

fn clean_effort(effort: Option<String>) -> Result<Option<String>> {
    match effort.map(|e| e.trim().to_lowercase()).filter(|e| !e.is_empty()) {
        None => Ok(None),
        Some(e) if EFFORTS.contains(&e.as_str()) => Ok(Some(e)),
        Some(e) => Err(Error::Validation(format!("Unknown effort '{e}'; use low, medium or high"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_are_the_start_of_the_first_line() {
        assert_eq!(title_from("Anything with Al Pacino?"), "Anything with Al Pacino?");
        let long = "Is there a series I should watch based on what I have seen lately, maybe something Korean";
        let title = title_from(long);
        assert!(title.ends_with('…') && title.chars().count() <= 61, "{title}");
        assert_eq!(title_from("First line\nsecond"), "First line");
    }
}
