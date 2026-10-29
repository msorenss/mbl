use mbl_core::{Actor, Error, LibraryId, Result, UserId, time::now_ms};
use mbl_db::Db;
use mbl_db::repo::m6::{self, UserSecurityInfo};
use mbl_db::repo::sessions::SessionsRepo;
use mbl_db::repo::users::{
    HomeLayoutRequest, NewUser, SubtitleStylePreferences, UpdateUserPreferences, User,
    UserPreferences, UsersRepo,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{blocking, validate};

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateUser {
    pub username: String,
    pub password: String,
    pub display_name: Option<String>,
    pub is_admin: bool,
    #[serde(default)]
    pub is_managed: bool,
    #[serde(default)]
    pub content_rating_ceiling: Option<String>,
    #[serde(default)]
    pub library_ids: Option<Vec<LibraryId>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateUser {
    pub display_name: Option<String>,
    pub password: Option<String>,
    pub is_admin: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateUserPermissionsRequest {
    pub is_managed: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_rating_patch")]
    pub content_rating_ceiling: Option<Option<String>>,
    pub library_ids: Option<Vec<LibraryId>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetPinRequest {
    /// 4-8 numeric digits, or null to remove PIN.
    pub pin: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct VerifyPinRequest {
    pub pin: String,
}

// A missing field means unchanged; explicit JSON null removes the limit.
fn deserialize_rating_patch<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Option<Option<String>>, D::Error> {
    Option::<String>::deserialize(deserializer).map(Some)
}

#[cfg(test)]
mod permission_patch_tests {
    use super::*;
    #[tokio::test]
    async fn all_five_locales_are_persisted_and_unknown_locale_is_rejected() {
        let directory = tempfile::tempdir().unwrap();
        let path = camino::Utf8Path::from_path(directory.path()).unwrap().join("preferences.db");
        let db = Db::connect(&path, &mbl_db::DbOptions::default()).await.unwrap();
        let id = UsersRepo::new(db.clone())
            .insert(
                &NewUser {
                    username: "viewer",
                    display_name: "Viewer",
                    password_hash: "test",
                    is_admin: false,
                },
                now_ms(),
            )
            .await
            .unwrap();
        let service = UserService::new(db);
        let actor = Actor::user(id);
        for locale in ["en", "sv", "de", "fr", "es"] {
            let result = service
                .update_preferences(
                    &actor,
                    UpdateUserPreferences { locale: Some(locale.into()), ..Default::default() },
                )
                .await
                .unwrap();
            assert_eq!(result.locale, locale);
            assert_eq!(service.get_preferences(&actor).await.unwrap().locale, locale);
        }
        assert!(
            service
                .update_preferences(
                    &actor,
                    UpdateUserPreferences { locale: Some("unknown".into()), ..Default::default() }
                )
                .await
                .is_err()
        );
        assert_eq!(service.get_preferences(&actor).await.unwrap().locale, "es");
    }

    #[test]
    fn rating_patch_distinguishes_omitted_null_and_value() {
        let absent: UpdateUserPermissionsRequest = serde_json::from_str("{}").unwrap();
        let cleared: UpdateUserPermissionsRequest =
            serde_json::from_str(r#"{"contentRatingCeiling":null}"#).unwrap();
        let limited: UpdateUserPermissionsRequest =
            serde_json::from_str(r#"{"contentRatingCeiling":"PG"}"#).unwrap();
        assert_eq!(absent.content_rating_ceiling, None);
        assert_eq!(cleared.content_rating_ceiling, Some(None));
        assert_eq!(limited.content_rating_ceiling, Some(Some("PG".into())));
    }
}

const VALID_THEMES: &[&str] = &["dark", "oled", "light", "high-contrast"];
const VALID_ACCENTS: &[&str] = &["amber", "indigo", "emerald", "rose", "cyan", "violet"];
const VALID_LOCALES: &[&str] = &["en", "sv", "de", "fr", "es"];
const VALID_SUB_SIZES: &[&str] = &["sm", "md", "lg", "xl"];
const VALID_SUB_BACKGROUNDS: &[&str] = &["none", "dim", "solid"];
const VALID_SUB_COLORS: &[&str] = &["white", "yellow", "cyan"];

#[derive(Debug, Clone)]
pub struct UserService {
    db: Db,
    users: UsersRepo,
    sessions: SessionsRepo,
}

impl UserService {
    pub fn new(db: Db) -> Self {
        Self { users: UsersRepo::new(db.clone()), sessions: SessionsRepo::new(db.clone()), db }
    }

    pub async fn list(&self, actor: &Actor) -> Result<Vec<User>> {
        actor.require_admin()?;
        self.users.list().await
    }

    /// List household user profiles for the profile switcher (any authenticated user).
    pub async fn list_profiles(&self, _actor: &Actor) -> Result<Vec<User>> {
        self.users.list().await
    }

    pub async fn get_preferences(&self, actor: &Actor) -> Result<UserPreferences> {
        self.users.get_preferences(actor.user_id).await?.ok_or(Error::NotFound)
    }

    pub async fn update_preferences(
        &self,
        actor: &Actor,
        patch: UpdateUserPreferences,
    ) -> Result<UserPreferences> {
        let mut prefs = self.get_preferences(actor).await?;
        if let Some(theme) = patch.theme {
            if !VALID_THEMES.contains(&theme.as_str()) {
                return Err(Error::Validation(format!("invalid theme: {theme}")));
            }
            prefs.theme = theme;
        }
        if let Some(accent) = patch.accent {
            if !VALID_ACCENTS.contains(&accent.as_str()) {
                return Err(Error::Validation(format!("invalid accent: {accent}")));
            }
            prefs.accent = accent;
        }
        if let Some(locale) = patch.locale {
            if !VALID_LOCALES.contains(&locale.as_str()) {
                return Err(Error::Validation(format!("invalid locale: {locale}")));
            }
            prefs.locale = locale;
        }
        if let Some(reduced_motion) = patch.reduced_motion {
            prefs.reduced_motion = reduced_motion;
        }
        if let Some(night_mode) = patch.night_mode {
            prefs.night_mode = night_mode;
        }
        if let Some(audio_offset_ms) = patch.audio_offset_ms {
            prefs.audio_offset_ms = audio_offset_ms.clamp(-10_000, 10_000);
        }
        if let Some(sub) = patch.subtitle_style {
            validate_subtitle_style(&sub)?;
            prefs.subtitle_style = sub;
        }
        if let Some(home_layout) = patch.home_layout {
            if home_layout.len() > 64 {
                return Err(Error::Validation("homeLayout has too many entries".into()));
            }
            prefs.home_layout = home_layout;
        }
        if let Some(hidden_hubs) = patch.hidden_hubs {
            if hidden_hubs.len() > 64 {
                return Err(Error::Validation("hiddenHubs has too many entries".into()));
            }
            prefs.hidden_hubs = hidden_hubs;
        }
        self.users.set_preferences(actor.user_id, &prefs).await?;
        Ok(prefs)
    }

    pub async fn update_home_layout(
        &self,
        actor: &Actor,
        req: HomeLayoutRequest,
    ) -> Result<UserPreferences> {
        self.update_preferences(
            actor,
            UpdateUserPreferences {
                home_layout: Some(req.home_layout),
                hidden_hubs: req.hidden_hubs,
                ..Default::default()
            },
        )
        .await
    }

    /// Admins can read anyone; users only themselves (others are reported as not found).
    pub async fn get(&self, actor: &Actor, id: UserId) -> Result<User> {
        if !actor.is_admin && actor.user_id != id {
            return Err(Error::NotFound);
        }
        self.users.get(id).await?.ok_or(Error::NotFound)
    }

    pub async fn create(&self, actor: &Actor, input: CreateUser) -> Result<User> {
        actor.require_admin()?;
        let username = validate::username(&input.username)?;
        validate::password(&input.password)?;
        let display_name = validate::display_name(input.display_name.as_deref(), &username)?;

        let password = input.password;
        let hash = blocking(move || mbl_auth::hash_password(&password)).await?;
        let id = self
            .users
            .insert(
                &NewUser {
                    username: &username,
                    display_name: &display_name,
                    password_hash: &hash,
                    is_admin: input.is_admin,
                },
                now_ms(),
            )
            .await?;

        if input.is_managed || input.content_rating_ceiling.is_some() || input.library_ids.is_some()
        {
            m6::update_user_permissions(
                &self.db,
                id,
                Some(input.is_managed),
                Some(input.content_rating_ceiling),
                input.library_ids,
            )
            .await?;
        }

        tracing::info!(user_id = %id, by = %actor.user_id, "user created");
        self.users.get(id).await?.ok_or(Error::NotFound)
    }

    /// Fetch security, managed user status, content rating ceiling, and library ACLs.
    pub async fn get_security_info(&self, actor: &Actor, id: UserId) -> Result<UserSecurityInfo> {
        if !actor.is_admin && actor.user_id != id {
            return Err(Error::NotFound);
        }
        m6::get_user_security_info(&self.db, id).await?.ok_or(Error::NotFound)
    }

    /// Update managed status, content rating ceiling, or library ACLs (admin only).
    pub async fn update_permissions(
        &self,
        actor: &Actor,
        id: UserId,
        req: UpdateUserPermissionsRequest,
    ) -> Result<UserSecurityInfo> {
        actor.require_admin()?;
        self.users.get(id).await?.ok_or(Error::NotFound)?;

        m6::update_user_permissions(
            &self.db,
            id,
            req.is_managed,
            req.content_rating_ceiling,
            req.library_ids,
        )
        .await?;

        self.get_security_info(actor, id).await
    }

    /// Set or clear a numeric PIN for a user profile (admin or self).
    pub async fn set_pin(&self, actor: &Actor, id: UserId, pin: Option<String>) -> Result<()> {
        if !actor.is_admin && actor.user_id != id {
            return Err(Error::NotFound);
        }
        let pin_hash = match pin {
            Some(ref p) if !p.trim().is_empty() => {
                let p = p.clone();
                Some(blocking(move || mbl_auth::hash_pin(&p)).await?)
            }
            _ => None,
        };

        m6::set_user_pin(&self.db, id, pin_hash.as_deref()).await?;
        tracing::info!(user_id = %id, by = %actor.user_id, has_pin = pin_hash.is_some(), "user PIN updated");
        Ok(())
    }

    /// Verify a candidate numeric PIN against the user's stored PIN hash.
    pub async fn verify_pin(&self, _actor: &Actor, id: UserId, pin: &str) -> Result<bool> {
        let hash = m6::get_user_pin_hash(&self.db, id).await?;
        let Some(stored_hash) = hash else {
            return Ok(true); // No PIN set
        };
        let pin = pin.to_string();
        blocking(move || Ok(mbl_auth::verify_pin(&pin, &stored_hash))).await
    }

    /// Users may change their own display name and password; only admins may change roles.
    pub async fn update(&self, actor: &Actor, id: UserId, patch: UpdateUser) -> Result<User> {
        let target = self.get(actor, id).await?;

        if let Some(is_admin) = patch.is_admin {
            actor.require_admin()?;
            if target.is_admin && !is_admin && self.users.count_admins().await? <= 1 {
                return Err(Error::Conflict("cannot demote the last admin".into()));
            }
        }
        let display_name = patch
            .display_name
            .as_deref()
            .map(|d| validate::display_name(Some(d), &target.username))
            .transpose()?;
        if let Some(p) = &patch.password {
            validate::password(p)?;
        }

        if let Some(name) = display_name {
            self.users.update_display_name(id, &name).await?;
        }
        if let Some(password) = patch.password {
            let hash = blocking(move || mbl_auth::hash_password(&password)).await?;
            self.users.update_password(id, &hash).await?;
        }
        if let Some(is_admin) = patch.is_admin {
            self.users.update_admin(id, is_admin).await?;
        }
        self.users.get(id).await?.ok_or(Error::NotFound)
    }

    pub async fn delete(&self, actor: &Actor, id: UserId) -> Result<()> {
        actor.require_admin()?;
        if actor.user_id == id {
            return Err(Error::Conflict("you cannot delete your own account".into()));
        }
        let target = self.users.get(id).await?.ok_or(Error::NotFound)?;
        if target.is_admin && self.users.count_admins().await? <= 1 {
            return Err(Error::Conflict("cannot delete the last admin".into()));
        }
        self.sessions.delete_all_for_user(id).await?;
        self.users.delete(id).await?;
        tracing::info!(user_id = %id, by = %actor.user_id, "user deleted");
        Ok(())
    }
}

fn validate_subtitle_style(sub: &SubtitleStylePreferences) -> Result<()> {
    if !VALID_SUB_SIZES.contains(&sub.size.as_str()) {
        return Err(Error::Validation(format!("invalid subtitle size: {}", sub.size)));
    }
    if !VALID_SUB_BACKGROUNDS.contains(&sub.background.as_str()) {
        return Err(Error::Validation(format!("invalid subtitle background: {}", sub.background)));
    }
    if !VALID_SUB_COLORS.contains(&sub.color.as_str()) {
        return Err(Error::Validation(format!("invalid subtitle color: {}", sub.color)));
    }
    Ok(())
}
