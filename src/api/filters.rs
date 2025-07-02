use crate::database::objects::{Session, User};
use crate::database::{Database, DatabaseError};
use axum::extract::{FromRequest, FromRequestParts, Request};
use axum::http::request::Parts;
use futures::{TryFutureExt, TryStreamExt};
use reqwest::StatusCode;
use sqlx::encode::IsNull::No;
use crate::api::serve::AppState;
use uuid::Uuid;
use crate::api::handlers::handle_database_error;

pub struct BearerToken(pub Uuid);

impl<S: Sync> FromRequestParts<S> for BearerToken {
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection>{
        let header = parts.headers.get("Authorization");
        if let Some(header) = header {
            if let Ok(header) = header.to_str() {
                if header[0..7] == *"Bearer " {
                    if let Ok(token) = Uuid::parse_str(&header[7..]) {
                        return Ok(BearerToken(token));
                    }
                }
            }
        }

        let cookies = parts.headers.get_all("Cookie");
        for cookie in cookies {
            if let Ok(cokie) = cookie.to_str() {
                if let Some((name, value)) = cokie.split_once("=") {
                    if name == "session-token" {
                        if let Ok(token) = Uuid::parse_str(value) {
                            return Ok(BearerToken(token));
                        }
                    }
                }
            }
        }

        Err(StatusCode::UNAUTHORIZED)

    }
}

pub struct WithSession(pub Session);

impl FromRequestParts<AppState> for WithSession {
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let token = BearerToken::from_request_parts(parts, state).await?;

        Ok(Self(state.get_where("token", token.0, None).await.map_err(handle_database_error)?))
    }
}

pub struct UserAuth(pub User);

impl FromRequestParts<AppState> for UserAuth {
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let session = WithSession::from_request_parts(parts, state).await?;

        Ok(Self(state.get_one(session.0.user_id, None).await.map_err(handle_database_error)?))
    }
}
