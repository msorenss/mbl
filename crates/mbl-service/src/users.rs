use mbl_core::{Actor, Error, Result, UserId, time::now_ms};
use mbl_db::Db;
use mbl_db::repo::sessions::SessionsRepo;
use mbl_db::repo::users::{NewUser, User, UsersRepo};

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
