use crate::api::filters::{BearerToken, FileUpload, UserAuth, WithSession};
use crate::api::serve::AppState;
use crate::api::{auth, filters};
use crate::config::CONFIG;
use crate::database::DatabaseError::SqlxError;
use crate::database::objects::{DbObject, FromJson, InviteLink, Session, UpdateJson, User, World};
use crate::database::types::Id;
use crate::database::{Cachable, DatabasePool, QueryBuilder, ValueType, WhereOperand};
pub(crate) use crate::database::{Database, DatabaseError};
use crate::{execute_on_enum, util};
use crate::util::base64::base64_decode;
use crate::util::dirs::icons_dir;
use async_trait::async_trait;
use axum::body::Bytes;
use axum::extract::{FromRequest, Multipart, Path, Request, State};
use axum::http::header::ToStrError;
use axum::http::{HeaderMap, StatusCode, header, HeaderName, HeaderValue};
use axum::response::IntoResponse;
use axum::routing::{MethodRouter, get, post};
use axum::{Json, Router};
use chrono::DateTime;
use futures::task::SpawnExt;
use image::imageops::{tile, FilterType};
use image::{DynamicImage, ImageFormat, ImageReader, Limits};
use log::{debug, error, info};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::query::Query;
use sqlx::{Encode, FromRow, IntoArguments, Type, query};
use std::fmt::format;
use std::fs::File;
use std::io::{BufWriter, Cursor, Read, Write};
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use tokio::io::BufReader;
use tokio::join;
use tokio::sync::Mutex;
use tokio::task::JoinSet;
use tokio::time::Instant;
use tokio_stream::StreamExt;
use tokio_util::io::ReaderStream;
use uuid::{Error, Uuid};

pub trait ApiObject: DbObject {
    fn routes() -> Router<AppState>;
}

#[derive(Debug, Deserialize)]
pub struct RecursiveQuery {
    pub recursive: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct PaginationQuery {
    pub page: Option<u32>,
    pub limit: Option<u32>,
}

pub struct PaginationSettings {
    pub page: u32,
    pub limit: u32,
}

impl PaginationQuery {
    pub fn unwrap(self) -> PaginationSettings {
        PaginationSettings {
            page: self.page.unwrap_or(0),
            limit: self.limit.unwrap_or(50).min(100),
        }
    }
}

impl Default for PaginationSettings {
    fn default() -> Self {
        Self { page: 0, limit: 50 }
    }
}

#[async_trait]
pub trait ApiList: ApiObject
where
    Self: Sized + 'static,
    Self: Serialize,
    for<'a> Self: FromRow<'a, sqlx::sqlite::SqliteRow>,
    for<'a> Self: FromRow<'a, sqlx::postgres::PgRow>,
    Self: Unpin,
    Self: Cachable,
{
    //in theory the user filter should be done within the sql query, but for the sake of simplicity we do that when collecting the results
    async fn api_list(
        State(state): State<AppState>,
        UserAuth(user): UserAuth,
        recursive: axum::extract::Query<RecursiveQuery>,
        pagination: axum::extract::Query<PaginationQuery>,
        axum::extract::Query(filters): axum::extract::Query<Vec<(String, String)>>,
    ) -> Result<impl IntoResponse, StatusCode> {
        let pagination = pagination.0.unwrap();

        let group = user.group(state.database.clone(), None).await;
        let objects: Vec<Self> = {
            execute_on_enum!(&state.database.pool; (DatabasePool::Postgres, DatabasePool::Sqlite) |pool| {
                let mut query = QueryBuilder::select::<Self>();
                for (column, value) in filters {
                    let (value, filter_type) = {
                        if let Some(value) = value.strip_prefix("!") {
                            (value.to_string(), WhereOperand::NotEqual)
                        } else if let Some(value) = value.strip_prefix("<=") {
                            (value.to_string(), WhereOperand::LessThanOrEqual)
                        } else if let Some(value) = value.strip_prefix(">=") {
                            (value.to_string(), WhereOperand::GreaterThanOrEqual)
                        } else if let Some(value) = value.strip_prefix("<") {
                            (value.to_string(), WhereOperand::LessThan)
                        } else if let Some(value) = value.strip_prefix(">") {
                            (value.to_string(), WhereOperand::GreaterThan)
                        } else {
                            (value, WhereOperand::Equal)
                        }
                    };

                    if let Some(column) = Self::get_column(&column) {
                        if !column.hidden {
                            if value.to_ascii_lowercase() == "null" && column.nullable {
                                match filter_type {
                                    WhereOperand::Equal => {
                                        query.where_null(column.name());
                                    }
                                    WhereOperand::NotEqual => {
                                        query.where_not_null(column.name());
                                    }
                                    _ => {
                                        //what do you mean you want "less than or equal to null"?
                                    }
                                }
                            } else {
                                match column.data_type {
                                    ValueType::Id => {
                                        if let Ok(value) = Id::from_string(&value) {
                                            query.where_operand(
                                                column.name(),
                                                value,
                                                filter_type,
                                            );
                                        }
                                    }
                                    ValueType::Token => {
                                        if let Ok(value) = Uuid::from_str(&value) {
                                            query.where_operand(
                                                column.name(),
                                                value,
                                                filter_type,
                                            );
                                        }
                                    }
                                    ValueType::Datetime => {
                                        if let Ok(value) = DateTime::parse_from_rfc3339(&value)
                                        {
                                            query.where_operand(
                                                column.name(),
                                                value,
                                                filter_type,
                                            );
                                        }
                                    }
                                    ValueType::Float => {
                                        if let Ok(value) = f32::from_str(&value) {
                                            query.where_operand(
                                                column.name(),
                                                value,
                                                filter_type,
                                            );
                                        }
                                    }
                                    ValueType::Integer => {
                                        if let Ok(value) = i64::from_str(&value) {
                                            query.where_operand(
                                                column.name(),
                                                value,
                                                filter_type,
                                            );
                                        }
                                    }
                                    ValueType::Boolean => {
                                        if let Ok(value) = bool::from_str(&value) {
                                            query.where_operand(
                                                column.name(),
                                                value,
                                                filter_type,
                                            );
                                        }
                                    }
                                    // this may work :shrug:
                                    ValueType::Blob => {
                                        if let Ok(value) = base64_decode(&value) {
                                            query.where_operand(
                                                column.name(),
                                                value,
                                                filter_type,
                                            );
                                        }
                                    }
                                    ValueType::Text => {
                                        query.where_operand(column.name(), value, filter_type)
                                    }
                                }
                            }
                        }
                    }
                }

                query.user_group::<Self>(&user, &group);

                query.pagination::<Self>(pagination);

                query
                    .query_builder
                    .build_query_as()
                    .fetch_all(pool)
                    .await
                    .map_err(DatabaseError::from)
                    .map_err(handle_database_error)?
            })
        };

        if recursive.recursive.unwrap_or(false) {
            let start = Instant::now();
            let mut values = Vec::new();

            // blocking, because asynchronous implementation would result in more cache misses
            for object in objects.into_iter() {
                state.database.cache.insert(object.clone()).await;
                values.push(
                    state
                        .database
                        .get_recursive::<Self>(object.id(), Some((&user, &group)))
                        .await,
                );
            }

            return Ok(Json(
                values
                    .into_iter()
                    .filter_map(|value| value.ok())
                    .collect::<Vec<_>>(),
            ));
        }

        Ok(axum::Json(
            objects
                .into_iter()
                .filter_map(|object| serde_json::to_value(object).ok())
                .collect::<Vec<_>>(),
        ))
    }
}

#[async_trait]
pub trait ApiGet: ApiObject
where
    Self: Sized + 'static,
    Self: Serialize,
    for<'a> Self: FromRow<'a, sqlx::sqlite::SqliteRow>,
    for<'a> Self: FromRow<'a, sqlx::postgres::PgRow>,
    Self: Unpin,
    Self: Cachable,
{
    async fn api_get(
        Path(id): Path<Id>,
        axum::extract::Query(recursive): axum::extract::Query<RecursiveQuery>,
        State(state): State<AppState>,
        UserAuth(user): UserAuth,
    ) -> Result<impl IntoResponse, StatusCode> {
        let group = user.group(state.database.clone(), None).await;
        Ok(Json(if recursive.recursive.unwrap_or(false) {
            state
                .database
                .get_recursive::<Self>(id, Some((&user, &group)))
                .await
                .map_err(handle_database_error)?
        } else {
            serde_json::to_value(
                state
                    .database
                    .get_one::<Self>(id, Some((&user, &group)))
                    .await
                    .map_err(handle_database_error)?,
            )
            .unwrap()
        }))
    }
}

#[async_trait]
pub trait ApiCreate: ApiObject + FromJson
where
    Self: Sized + 'static,
    Self: Serialize,
    Self: Clone,
    Self: for<'a> IntoArguments<'a, sqlx::Sqlite>,
    Self: for<'r> FromRow<'r, sqlx::sqlite::SqliteRow>,
    Self: for<'a> IntoArguments<'a, sqlx::Postgres>,
    Self: for<'r> FromRow<'r, sqlx::postgres::PgRow>,
    Self: Unpin,
    Self: Cachable,
{
    async fn api_create(
        recursive: axum::extract::Query<RecursiveQuery>,
        State(state): State<AppState>,
        UserAuth(user): UserAuth,
        Json(data): Json<Self::JsonFrom>,
    ) -> Result<impl IntoResponse, StatusCode> {
        let mut data = data;
        let group = user.group(state.database.clone(), None).await;

        if !Self::can_create(&user, &group) {
            return Err(StatusCode::UNAUTHORIZED);
        }

        let object = {
            debug!("running before create for /{}", Self::table_name());
            Self::before_api_create(state.clone(), &mut data, &user)
                .await
                .map_err(handle_database_error)?;
            let object = Self::from_json(&data, &user);
            let _ = state
                .database
                .insert(&object, Some((&user, &group)))
                .await
                .map_err(handle_database_error)?;

            debug!(
                "running after create for /{}/{}",
                Self::table_name(),
                object.id()
            );
            object
                .after_api_create(state.clone(), &mut data, &user)
                .await
                .map_err(handle_database_error)?;
            object
        };

        if recursive.recursive.unwrap_or(false) {
            return Ok(axum::Json(
                state
                    .database
                    .get_recursive::<Self>(object.id(), Some((&user, &group)))
                    .await
                    .map_err(handle_database_error)?,
            ));
        }

        Ok(axum::Json(serde_json::to_value(object).unwrap()))
    }

    #[allow(unused)]
    /// runs before the database entry creation
    async fn before_api_create(
        state: AppState,
        json: &mut Self::JsonFrom,
        user: &User,
    ) -> Result<(), DatabaseError> {
        Ok(())
    }
    #[allow(unused)]
    /// runs after the database entry creation
    ///
    /// this returns a [`Result`], but there is no mechanism to undo the entry creation. if this fails it should probably cause the program to panic
    async fn after_api_create(
        &self,
        state: AppState,
        json: &mut Self::JsonFrom,
        user: &User,
    ) -> Result<(), DatabaseError> {
        Ok(())
    }
}

#[async_trait]
pub trait ApiUpdate: ApiObject + UpdateJson
where
    Self: Sized + 'static,
    Self: serde::Serialize,
    Self: for<'a> IntoArguments<'a, sqlx::Sqlite>,
    Self: for<'r> FromRow<'r, sqlx::sqlite::SqliteRow>,
    Self: for<'a> IntoArguments<'a, sqlx::Postgres>,
    Self: for<'r> FromRow<'r, sqlx::postgres::PgRow>,
    Self: Unpin,
    Self: Clone,
    Self: Cachable,
{
    async fn api_update(
        Path(id): Path<Id>,
        recursive: axum::extract::Query<RecursiveQuery>,
        State(state): State<AppState>,
        UserAuth(user): UserAuth,
        Json(data): axum::Json<Self::JsonUpdate>,
    ) -> Result<impl IntoResponse, StatusCode> {
        let mut data = data;
        let group = user.group(state.database.clone(), None).await;

        let object = state
            .database
            .get_one::<Self>(id, Some((&user, &group)))
            .await
            .map_err(handle_database_error)?;

        debug!(
            "running before update for /{}/{}",
            Self::table_name(),
            object.id()
        );
        object
            .before_api_update(state.clone(), &mut data, &user)
            .await
            .map_err(handle_database_error)?;

        let object = object.update_with_json(&data);

        let _ = state
            .database
            .update(&object, Some((&user, &group)))
            .await
            .map_err(handle_database_error)?;

        debug!(
            "running after update for /{}/{}",
            Self::table_name(),
            object.id()
        );
        object
            .after_api_update(state.clone(), &mut data, &user)
            .await
            .map_err(handle_database_error)?;

        if recursive.recursive.unwrap_or(false) {
            return Ok(axum::Json(
                state
                    .database
                    .get_recursive::<Self>(object.id(), Some((&user, &group)))
                    .await
                    .map_err(handle_database_error)?,
            ));
        }

        Ok(axum::Json(serde_json::to_value(object).unwrap()))
    }
    #[allow(unused)]
    /// runs before the database entry update
    async fn before_api_update(
        &self,
        state: AppState,
        json: &mut Self::JsonUpdate,
        user: &User,
    ) -> Result<(), DatabaseError> {
        Ok(())
    }
    #[allow(unused)]
    /// runs after the database entry update
    ///
    /// this returns a [`Result`], but there is no mechanism to undo the entry update. if this fails it should probably cause the program to panic
    async fn after_api_update(
        &self,
        state: AppState,
        json: &mut Self::JsonUpdate,
        user: &User,
    ) -> Result<(), DatabaseError> {
        Ok(())
    }
}

#[async_trait]
pub trait ApiRemove: ApiObject
where
    Self: Sized + 'static,
    Self: for<'r> FromRow<'r, sqlx::sqlite::SqliteRow>,
    Self: for<'r> FromRow<'r, sqlx::postgres::PgRow>,
    Self: serde::Serialize,
    Self: Unpin,
    Self: Cachable,
{
    async fn api_remove(
        Path(id): Path<Id>,
        State(state): State<AppState>,
        UserAuth(user): UserAuth,
    ) -> Result<StatusCode, StatusCode> {
        let group = user.group(state.database.clone(), None).await;

        let object = state
            .database
            .get_one::<Self>(id, Some((&user, &group)))
            .await
            .map_err(handle_database_error)?;

        debug!(
            "running before delete for /{}/{}",
            Self::table_name(),
            object.id()
        );
        object
            .before_api_delete(state.clone(), &user)
            .await
            .map_err(handle_database_error)?;

        let _ = state
            .database
            .remove(&object, Some((&user, &group)))
            .await
            .map_err(handle_database_error)?;

        debug!(
            "running after delete for /{}/{}",
            Self::table_name(),
            object.id()
        );
        object
            .after_api_delete(state.clone(), &user)
            .await
            .map_err(handle_database_error)?;

        Ok(StatusCode::NO_CONTENT)
    }

    #[allow(unused)]
    /// runs before the database entry deletion
    async fn before_api_delete(&self, state: AppState, user: &User) -> Result<(), DatabaseError> {
        Ok(())
    }
    #[allow(unused)]
    /// runs after the database entry deletion
    ///
    /// this returns a [`Result`], but there is no mechanism to undo the entry deletion. if this fails it should probably cause the program to panic
    async fn after_api_delete(&self, state: AppState, user: &User) -> Result<(), DatabaseError> {
        Ok(())
    }
}

#[async_trait]
pub trait ApiIcon: ApiObject
where
    Self: Sized + 'static,
    Self: for<'r> FromRow<'r, sqlx::sqlite::SqliteRow>,
    Self: for<'r> FromRow<'r, sqlx::postgres::PgRow>,
    Self: serde::Serialize,
    Self: Unpin,
    Self: Cachable,
{
    async fn upload_icon(
        state: State<AppState>,
        id: Path<Id>,
        user: UserAuth,
        headers: HeaderMap,
        mut file: FileUpload,
    ) -> Result<impl IntoResponse, StatusCode> {
        let state = state.0;
        let id = id.0;
        let user = user.0;

        //check if the asset exists and the user has access to it
        let self_ = state
            .database
            .get_one::<Self>(
                id,
                Some((&user, &user.group(state.database.clone(), None).await)),
            )
            .await
            .map_err(|_| StatusCode::NOT_FOUND)?;

        let (bytes, image_format) = (
            file.bytes,
            ImageFormat::from_mime_type(file.content_type.essence_str()),
        );
        if image_format.is_none() {
            return Err(StatusCode::BAD_REQUEST);
        }
        let image_format = image_format.unwrap();

        let is_gif = match image_format {
            ImageFormat::Jpeg | ImageFormat::Png | ImageFormat::WebP | ImageFormat::Bmp => false,
            ImageFormat::Gif => true,
            _ => return Err(StatusCode::BAD_REQUEST),
        };

        let gif_path = crate::util::dirs::icons_dir()
            .join(Self::table_name())
            .join(format!("{}.gif", id));
        let webp_path = crate::util::dirs::icons_dir()
            .join(Self::table_name())
            .join(format!("{}.webp", id));

        if is_gif {
            // i think this is to verify if the image is valid
            let _ = bytes_to_image(bytes.clone(), image_format)?;

            let image_file =
                std::fs::File::create(&gif_path).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let mut writer = BufWriter::new(image_file);

            if webp_path.exists() {
                std::fs::remove_file(&webp_path).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            }

            // after making sure the file is a valid image write the original, as we don't want to alter a gif
            writer
                .write_all(bytes.as_ref())
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        } else {
            let image = bytes_to_image(bytes, image_format)?;
            let image = crop_image_to_square(&image);

            let after_icon_update_future = self_.after_icon_update(state.clone(), &user, &image);

            let image = image.resize(256, 256, FilterType::CatmullRom);

            let image_file =
                std::fs::File::create(&webp_path).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let mut writer = BufWriter::new(image_file);

            if gif_path.exists() {
                std::fs::remove_file(&gif_path).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            }

            image
                .write_to(&mut writer, ImageFormat::WebP)
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

            after_icon_update_future.await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }

        Ok(StatusCode::OK)
    }

    #[allow(unused)]
    async fn after_icon_update(&self, state: AppState, user: &User, image: &DynamicImage) -> Result<(), DatabaseError> {
        Ok(())
    }

    async fn get_icon(
        id: Path<Id>,
        _user: UserAuth, /*check if the user is authenticated, but do not check if they have access to the object, since it doesn't justify the extra DB lookups*/
    ) -> Result<impl IntoResponse, StatusCode> {
        let path = icons_dir().join(Self::table_name());
        let (path, is_gif) = if path.join(format!("{}.webp", *id)).exists() {
            (path.join(format!("{}.webp", *id)), false)
        } else if path.join(format!("{}.gif", *id)).exists() {
            (path.join(format!("{}.gif", *id)), true)
        } else {
            return Ok((
                StatusCode::SEE_OTHER,
                [(header::LOCATION, format!("/api/{}/default/icon", Self::table_name()))]
            ).into_response());
        };

        let file = tokio::fs::File::open(path)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        let stream = ReaderStream::new(file);
        let body = axum::body::Body::from_stream(stream);

        let mime_type = if is_gif { "image/gif" } else { "image/webp" };

        let header = [(header::CONTENT_TYPE, mime_type)];

        Ok((header, body).into_response())
    }

    const DEFAULT_ICON_BYTES: &'static [u8];
    const DEFAULT_ICON_MIME: &'static str;
    #[allow(clippy::unused_async)]
    async fn default_icon(
        _user: UserAuth, /*check if the user is authenticated, but do not check if they have access to the object, since it doesn't justify the extra DB lookups*/
        header_map: HeaderMap
    ) -> Result<impl IntoResponse, StatusCode> {
        if let Some(date) = header_map.get(header::IF_MODIFIED_SINCE) {
            if let Ok(date) = DateTime::parse_from_rfc2822(date.to_str().expect("invalid header value")) {
                if date < *util::START_TIME {
                    return Ok(StatusCode::NOT_MODIFIED.into_response());
                }
            }
        }

        let headers = (
            [(header::CONTENT_TYPE, Self::DEFAULT_ICON_MIME)],
            [(header::LAST_MODIFIED, util::START_TIME.to_rfc2822())]
        );
        Ok((headers, Self::DEFAULT_ICON_BYTES).into_response())
    }
}

pub(crate) fn handle_database_error(err: DatabaseError) -> StatusCode {
    match err {
        DatabaseError::Unauthorized => StatusCode::UNAUTHORIZED,
        DatabaseError::NotFound => StatusCode::NOT_FOUND,
        DatabaseError::InternalServerError(err) => {
            error!("{err}");
            StatusCode::INTERNAL_SERVER_ERROR
        }
        DatabaseError::Conflict => StatusCode::CONFLICT,
        DatabaseError::SqlxError(err) => match err {
            sqlx::Error::RowNotFound => StatusCode::NOT_FOUND,
            _ => {
                error!("{err}");
                StatusCode::INTERNAL_SERVER_ERROR
            }
        },
    }
}

#[derive(Serialize)]
struct TokenReply {
    token: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Login {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RegisterQuery {
    token: Option<String>,
}

pub async fn user_register(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<RegisterQuery>,
    Json(credentials): axum::extract::Json<Login>,
) -> Result<impl IntoResponse, StatusCode> {
    let token = query.token;
    //arbitrary values but who cares (foreshadowing)
    const ALLOWED_USERNAME_CHARS: &str =
        "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ012345789-_";
    const ALLOWED_PASSWORD_CHARS: &str =
        "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ012345789-._~:/?#[]@!$&'()*+,;%= ";
    if credentials.username.is_empty()
        || credentials.password.is_empty()
        || credentials.username.len() > 32
        || credentials.password.len() > 256
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    //TODO: do better username and password validation

    for char in credentials.username.chars() {
        if !ALLOWED_USERNAME_CHARS.contains(char) {
            return Err(StatusCode::BAD_REQUEST);
        }
    }
    for char in credentials.password.chars() {
        if !ALLOWED_PASSWORD_CHARS.contains(char) {
            return Err(StatusCode::BAD_REQUEST);
        }
    }

    let mut can_continue = false;
    let invite = if let Some(token) = token {
        let invite: Result<InviteLink, _> =
            state.database.get_where("invite_token", token, None).await;
        if let Ok(invite) = invite {
            can_continue = true;
            Some(invite)
        } else {
            None
        }
    } else {
        None
    };
    if !CONFIG.require_invite_to_register {
        can_continue = true;
    }

    if !can_continue {
        return Err(StatusCode::UNAUTHORIZED);
    }

    if state
        .database
        .get_where::<User, _>("username", &credentials.username, None)
        .await
        .is_ok()
    {
        return Err(StatusCode::CONFLICT);
    }

    let user = state
        .database
        .create_user(&credentials.username, &credentials.password)
        .await
        .map_err(|err| {
            error!("{err}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    if let Some(invite) = invite {
        state.database.remove(&invite, None).await.map_err(|err| {
            error!("{err}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    }

    Ok(StatusCode::CREATED)
}

//this in theory could be transformed into ApiCreate implementation, but it would require a fair amount of changes, and for now it's not causing any problems
#[allow(clippy::unused_async)]
pub async fn user_auth(
    database: State<AppState>,
    credentials: axum::extract::Json<Login>,
) -> Result<impl IntoResponse, StatusCode> {
    let database = database.0;
    let credentials = credentials.0;

    let session = {
        let database = database;

        auth::try_user_auth(&credentials.username, &credentials.password, database)
            .await
            .map_err(handle_database_error)?
    };
    Ok((
        StatusCode::CREATED,
        [(header::ACCESS_CONTROL_ALLOW_CREDENTIALS, "true")],
        [(
            header::SET_COOKIE,
            format!(
                "session-token={}; Path=/api; HttpOnly; Max-Age=1209600; charset=UTF-8",
                session.token.as_simple().to_string()
            ),
        )],
        axum::Json(json!({"token": session.token.as_simple().to_string()})),
    ))
}

#[allow(clippy::unused_async)]
pub async fn logout(
    state: State<AppState>,
    session: WithSession,
) -> Result<StatusCode, StatusCode> {
    let state = state.0;
    let session = session.0;

    state
        .database
        .remove(&session, None)
        .await
        .map_err(handle_database_error)?;

    Ok(StatusCode::OK)
}

#[allow(clippy::unused_async)]
#[axum::debug_handler]
pub async fn user_info(
    user: UserAuth,
    axum::extract::Query(recursive): axum::extract::Query<RecursiveQuery>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, StatusCode> {
    Ok(axum::Json(if recursive.recursive.unwrap_or(false) {
        state
            .database
            .get_recursive::<User>(user.0.id, None)
            .await
            .unwrap()
    } else {
        serde_json::to_value(user.0).unwrap()
    }))
}

#[allow(clippy::unused_async)]
pub async fn server_info() -> Result<impl IntoResponse, StatusCode> {
    #[derive(Serialize)]
    struct ServerInfo {
        name: String,
        login_message: String,
        login_message_title: String,
        login_message_type: String,
        requires_invite: bool,
        world: WorldInfo,
    }
    #[derive(Serialize)]
    struct WorldInfo {
        min_memory: u32,
        default_memory: u32,
        hostname: String,
        port: u16,
    }

    Ok(axum::Json(ServerInfo {
        name: CONFIG.info.name.clone(),
        login_message: CONFIG.info.login_message.clone(),
        login_message_title: CONFIG.info.login_message_title.clone(),
        login_message_type: CONFIG.info.login_message_type.clone(),
        requires_invite: CONFIG.require_invite_to_register,
        world: WorldInfo {
            min_memory: CONFIG.world.minimum_memory,
            default_memory: CONFIG.world_defaults.allocated_memory,
            hostname: CONFIG.proxy.hostname.clone(),
            port: CONFIG.proxy.port,
        },
    }))
}

#[allow(clippy::unused_async)]
#[axum::debug_handler]
pub async fn generate_console_ticket(
    WithSession(session): WithSession,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, StatusCode> {
    #[derive(Serialize)]
    struct Ticket {
        ticket: Uuid
    }
    let ticket = Uuid::new_v4();
    
    state.console_tickets.insert(ticket, session.id).await;
    
    Ok(Json(Ticket {ticket}))
}

pub async fn get_username_valid(
    username: Path<String>,
    database: State<AppState>,
) -> impl IntoResponse {
    if database
        .database
        .get_where::<User, _>("username", username.0, None)
        .await
        .is_ok()
    {
        Json(json!({"valid": false}))
    } else {
        Json(json!({"valid": true}))
    }
}
pub async fn get_invite_valid(
    Path(invite_link): Path<Uuid>,
    State(database): State<AppState>,
) -> impl IntoResponse {
    if database
        .database
        .get_where::<InviteLink, _>("invite_token", invite_link, None)
        .await
        .is_ok()
    {
        Json(json!({"valid": true}))
    } else {
        Json(json!({"valid": false}))
    }
}

// user auth not needed, but unauthenticated users should not access this route
pub async fn get_hostname_valid(
    Path(hostname): Path<String>,
    State(state): State<AppState>,
    _: UserAuth,
) -> impl IntoResponse {
    if state
        .database
        .get_where::<World, _>("hostname", hostname, None)
        .await
        .is_ok()
    {
        Json(json!({"valid": false}))
    } else {
        Json(json!({"valid": true}))
    }
}

fn bytes_to_image(bytes: Bytes, format: ImageFormat) -> Result<DynamicImage, StatusCode> {
    let mut reader = ImageReader::new(Cursor::new(bytes.as_ref()));

    let mut limits = Limits::default();
    limits.max_image_width = Some(4000);
    limits.max_image_height = Some(4000);

    reader.set_format(format);

    let mut file =
        std::fs::File::create(PathBuf::from_str("/home/mhanak/uploaded.png").unwrap()).unwrap();
    file.write_all(bytes.as_ref()).unwrap();

    reader.limits(limits);
    let image = reader.decode().map_err(|err| {
        error!("{err}");
        StatusCode::BAD_REQUEST
    })?;

    Ok(image)
}

fn crop_image_to_square(image: &DynamicImage) -> DynamicImage {
    let delta = image.width().abs_diff(image.height());
    if image.width() > image.height() {
        image.crop_imm(delta / 2, 0, image.height(), image.height())
    } else {
        image.crop_imm(0, delta / 2, image.width(), image.width())
    }


}
