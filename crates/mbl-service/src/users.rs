use mbl_core::{Actor, Error, Result, UserId, time::now_ms};
use mbl_db::Db;
use mbl_db::repo::sessions::SessionsRepo;
use mbl_db::repo::users::{
    HomeLayoutRequest, NewUser, SubtitleStylePreferences, UpdateUserPreferences, User,
    UserPreferences, UsersRepo,
};

use crate::{blocking, validate};

#[derive(Debug, Clone)]
pub struct CreateUser {
    pub username: String,
    pub password: String,
    pub display_name: Option<String>,
    pub is_admin: bool,
}

#[derive(Debug, Clone, Default)]
pub struct UpdateUser {
    pub display_name: Option<String>,
    pub password: Option<String>,
    pub is_admin: Option<bool>,
}

const VALID_THEMES: &[&str] = &["dark", "oled", "light", "high-contrast"];
const VALID_ACCENTS: &[&str] = &["amber", "indigo", "emerald", "rose", "cyan", "violet"];
const VALID_LOCALES: &[&str] = &["en", "sv", "de"];
const VALID_SUB_SIZES: &[&str] = &["sm", "md", "lg", "xl"];
const VALID_SUB_BACKGROUNDS: &[&str] = &["none", "dim", "solid"];
const VALID_SUB_COLORS: &[&str] = &["white", "yellow", "cyan"];

#[derive(Debug, Clone)]
pub struct UserService {
    users: UsersRepo,
    sessions: SessionsRepo,
}

impl UserService {
    pub fn new(db: Db) -> Self {
        Self { users: UsersRepo::new(db.clone()), sessions: SessionsRepo::new(db) }
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
        tracing::info!(user_id = %id, by = %actor.user_id, "user created");
        self.users.get(id).await?.ok_or(Error::NotFound)
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
