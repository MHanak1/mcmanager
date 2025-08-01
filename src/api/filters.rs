use crate::api::handlers::handle_database_error;
use crate::api::serve::AppState;
use crate::database::objects::{Session, User};
use axum::body::Bytes;
use axum::extract::{FromRequest, FromRequestParts, Multipart, Request};
use axum::http::request::Parts;
use log::debug;
use mime::Mime;
use reqwest::StatusCode;
use std::str::FromStr;
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
                        debug!("found session token in header: {token}");
                        return Ok(BearerToken(token));
                    }
                }
            }
        }

        let cookies = axum_extra::extract::CookieJar::from_request_parts(parts, state).await.unwrap();

        if let Some(cookie) = cookies.get("session-token") {
            if let Ok(token) = Uuid::parse_str(cookie.value()) {
                debug!("found session token in cookie: {token}");
                return Ok(BearerToken(token));
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
                debug!("error getting session: {err}");
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
            .get_one::<User>(session.0.user_id, None)
            .await
            .map_err(handle_database_error)
        {
            Ok(user) => {
                debug!("found user: {}", user.id);
                Ok(Self(user))
            }
            Err(err) => {
                debug!("error getting user: {err}");
                Err(StatusCode::UNAUTHORIZED)
            }
        }
    }
}

pub struct FileUpload {
    pub bytes: Bytes,
    pub content_type: Mime,
}

impl<S: std::marker::Send + std::marker::Sync> FromRequest<S> for FileUpload {
    type Rejection = StatusCode;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let mut multipart = Multipart::from_request(req, state)
            .await
            .map_err(|_| StatusCode::BAD_REQUEST)?;

        while let Some(field) = multipart.next_field().await.unwrap() {
            if let Some("file") = field.name() {
                let content_type = if let Some(content_type) = field.content_type() {
                    Mime::from_str(content_type).map_err(|_| StatusCode::BAD_REQUEST)?
                } else {
                    continue;
                };

                let field_bytes = if let Ok(bytes) = field.bytes().await {
                    bytes
                } else {
                    continue;
                };

                return Ok(Self {
                    bytes: field_bytes,
                    content_type,
                });
            }
        }

        Err(StatusCode::BAD_REQUEST)
    }
}
