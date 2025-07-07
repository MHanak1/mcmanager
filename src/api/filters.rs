use crate::api::handlers::handle_database_error;
use crate::api::serve::AppState;
use crate::database::objects::{Session, User};
use crate::database::{Database, DatabaseError};
use axum::extract::{FromRequest, FromRequestParts, Request};
use axum::http::request::Parts;
use futures::{TryFutureExt, TryStreamExt};
use log::debug;
use reqwest::StatusCode;
use sqlx::encode::IsNull::No;
use uuid::Uuid;

pub struct BearerToken(pub Uuid);

impl<S: Sync + std::marker::Send> FromRequestParts<S> for BearerToken {
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let header = parts.headers.get("Authorization");
        if let Some(header) = header {
            if let Ok(header) = header.to_str() {
                if header[0..7] == *"Bearer " {
                    if let Ok(token) = Uuid::parse_str(&header[7..]) {
                        debug!("found session token in header: {}", token);
                        return Ok(BearerToken(token));
                    }
                }
            }
        }

        if let Ok(cookies) = axum_extra::extract::CookieJar::from_request_parts(parts, state).await
        {
            if let Some(cookie) = cookies.get("session-token") {
                if let Ok(token) = Uuid::parse_str(cookie.value()) {
                    debug!("found session token in cookie: {}", token);
                    return Ok(BearerToken(token));
                }
            }
        }

        debug!("auth token not found");
        Err(StatusCode::UNAUTHORIZED)
    }
}

pub struct WithSession(pub Session);

impl FromRequestParts<AppState> for WithSession {
    type Rejection = StatusCode;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let token = BearerToken::from_request_parts(parts, state).await?;

        match state
            .database
            .get_session(token.0, None)
            .await
            .map_err(handle_database_error)
        {
            Ok(session) => {
                debug!("found session: {}", session.id);
                Ok(Self(session))
            }
            Err(err) => {
                debug!("error getting session: {}", err);
                Err(StatusCode::UNAUTHORIZED)
            }
        }
    }
}

pub struct UserAuth(pub User);

impl FromRequestParts<AppState> for UserAuth {
    type Rejection = StatusCode;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let session = WithSession::from_request_parts(parts, state).await?;

        match state
            .database
            .get_user(session.0.user_id, None)
            .await
            .map_err(handle_database_error)
        {
            Ok(user) => {
                debug!("found user: {}", user.id);
                Ok(Self(user))
            }
            Err(err) => {
                debug!("error getting user: {}", err);
                Err(StatusCode::UNAUTHORIZED)
            }
        }
    }
}
