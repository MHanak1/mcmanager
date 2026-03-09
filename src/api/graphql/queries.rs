use std::time::Duration;

use argon2::{Argon2, PasswordHash, PasswordVerifier};
use color_eyre::Result;
use color_eyre::eyre::{ContextCompat, bail, eyre};
use log::debug;
use sea_orm::{
    ActiveModelTrait, DatabaseConnection, EntityTrait, IntoActiveModel, ModelTrait,
    TransactionTrait,
};
use seaography::CustomFields;
use tokio::time::Instant;
use tracing::error;
use uuid::Uuid;

use crate::api::graphql::types::CreatedSession;
use crate::app::state::AppState;
use crate::database::{
    Data, Group, InviteLink, PartialPassword, PartialUser, UserModel,
};
use crate::util::{sanitise_slug, validate_password, validate_username};
use crate::{
    database::{PartialSession, Password, SessionModel, User},
    util,
};

pub struct Operations;

#[CustomFields]
impl Operations {
    async fn register(
        ctx: &async_graphql::Context<'_>,
        username: String,
        password: String,
        invite_token: Option<Uuid>,
    ) -> async_graphql::Result<UserModel> {
        let state = ctx.data::<AppState>()?;

        let response_time = Instant::now()
            .checked_add(Duration::from_secs(1))
            .ok_or(async_graphql::Error::new("Internal Server Error"))?;

        let user = register(state, username, password, invite_token).await?;

        tokio::time::sleep_until(response_time).await;
        Ok(user)
    }

    async fn login(
        ctx: &async_graphql::Context<'_>,
        username: String,
        password: String,
        expires: Option<bool>,
    ) -> async_graphql::Result<CreatedSession> {
        let state = ctx.data::<AppState>()?;

        let reject_time = Instant::now()
            .checked_add(Duration::from_secs(1))
            .ok_or(async_graphql::Error::new("Internal Server Error"))?;

        let database = &state.database();

        let session = login(database, username, password, expires)
            .await
            .map_err(|err| {
                if err.to_string() == "Unauthorised" {
                    async_graphql::Error::new("Unauthorised")
                } else {
                    error!("Error while authenticating user: {err}");
                    async_graphql::Error::new("Internal Server Error")
                }
            });
        let session = match session {
            Ok(session) => Ok(session),
            Err(err) => {
                tokio::time::sleep_until(reject_time).await;
                Err(err)
            }
        }?;

        Ok(CreatedSession {
            id: session.id,
            user_id: session.user_id,
            token: session.token,
            created: session.created,
            expires: session.expires,
        })
    }
}

async fn register(
    state: &AppState,
    username: String,
    password: String,
    invite_token: Option<Uuid>,
) -> Result<UserModel> {
    let database = state.database();
    let config = state.config().get();
    if invite_token.is_none() && config.app.require_invite_for_account_creation {
        bail!("An invite token is required to create an account");
    }

    validate_username(&username)?;
    validate_password(&password)?;

    let transaction = database.begin().await?;

    if config.app.require_invite_for_account_creation
        && let Some(invite_token) = invite_token
    {
        let invite_link = InviteLink::find_by_token(invite_token)
            .one(&database)
            .await?;

        if let Some(invite_link) = invite_link {
            invite_link.delete(&database).await?;
        } else {
            bail!("Invalid invite token");
        }
    }

    if User::find_by_slug(sanitise_slug(&username))
        .one(&database)
        .await?
        .is_some()
    {
        bail!("Username already taken")
    }

    let group_slug = Data::find_by_id("default_group")
        .one(&database)
        .await?
        .context("Could not determine the default group")?
        .value
        .as_str()
        .expect("default_group entry is not a string")
        .to_owned();

    let group = Group::find_by_slug(group_slug)
        .one(&database)
        .await?
        .context("Default group missing (No group with the slug of {group_slug}")?;

    let user = PartialUser {
        slug: sanitise_slug(&username),
        username,
        group_id: group.id,
        ..Default::default()
    };

    let user = user.into_active_model().insert(&database).await?;

    let password = PartialPassword::new(user.id, &password)?;

    password.into_active_model().insert(&database).await?;

    transaction.commit().await?;

    Ok(user)
}

async fn login(
    database: &DatabaseConnection,
    username: String,
    password: String,
    expires: Option<bool>,
) -> Result<SessionModel> {
    let user = if let Some(user) = User::find_by_slug(util::sanitise_slug(&username))
        .one(database)
        .await?
    {
        user
    } else {
        debug!("Rejecting user auth: User not found");
        bail!("Unauthorised")
    };

    if user.username != username {
        debug!("Rejecting user auth: Found user by slug, invalid username");
        bail!("Unauthorised")
    }

    let database_password =
        if let Some(password) = Password::find_by_user_id(user.id).one(database).await? {
            password
        } else {
            debug!("Rejecting user auth: Password not set for user");
            bail!("Unauthorised")
        };

    let password_hash =
        PasswordHash::new(&database_password.hash).map_err(|err| eyre!(err.to_string()))?;

    if Argon2::default()
        .verify_password(password.as_bytes(), &password_hash)
        .is_ok()
    {
        let session = PartialSession {
            user_id: user.id,
            expires: expires.unwrap_or(false),
            ..Default::default()
        };

        let session = session.into_active_model().insert(database).await?;

        Ok(session)
    } else {
        debug!("Rejecting user auth: Invalid password");
        bail!("Unauthorised")
    }
}
