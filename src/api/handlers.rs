use crate::api::filters::{BearerToken, UserAuth, WithSession};
use crate::api::serve::AppState;
use crate::api::{auth, filters};
use crate::config::CONFIG;
use crate::database::DatabaseError::SqlxError;
use crate::database::objects::{DbObject, FromJson, InviteLink, Session, UpdateJson, User, World};
use crate::database::types::Id;
pub(crate) use crate::database::{Database, DatabaseError};
use crate::database::{DatabasePool, QueryBuilder, ValueType, WhereOperand};
use crate::execute_on_enum;
use crate::util::base64::base64_decode;
use crate::util::dirs::icons_dir;
use async_trait::async_trait;
use axum::body::Bytes;
use axum::extract::{FromRequest, Multipart, Path, Request, State};
use axum::http::header::ToStrError;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::IntoResponse;
use axum::routing::{MethodRouter, get, post};
use axum::{Json, Router};
use chrono::DateTime;
use futures::{StreamExt, TryFutureExt};
use image::error::UnsupportedErrorKind::Format;
use image::imageops::FilterType;
use image::{DynamicImage, ImageFormat, ImageReader, Limits};
use log::{debug, error};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{Encode, FromRow, IntoArguments, Type};
use std::fmt::format;
use std::fs::File;
use std::io::{BufWriter, Cursor, Read, Write};
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use tokio::io::BufReader;
use tokio::join;
use tokio::sync::Mutex;
use tokio_util::io::ReaderStream;
use uuid::{Error, Uuid};

pub trait ApiObject: DbObject {
    fn routes() -> Router<AppState>;
}

#[async_trait]
pub trait ApiList: ApiObject
where
    Self: Sized + 'static,
    Self: Serialize,
    for<'a> Self: FromRow<'a, sqlx::sqlite::SqliteRow>,
    for<'a> Self: FromRow<'a, sqlx::postgres::PgRow>,
    Self: Unpin,
{
    //in theory the user filter should be done within the sql query, but for the sake of simplicity we do that when collecting the results
    async fn api_list(
        database: State<AppState>,
        user: UserAuth,
        filters: axum::extract::Query<Vec<(String, String)>>,
    ) -> Result<axum::Json<Vec<Self>>, StatusCode> {
        let database = database.0;
        let user = user.0;
        let filters = filters.0;
        let group = user.group(database.clone(), None).await;
        let objects: Vec<Self> = {
            execute_on_enum!(&database.pool; (DatabasePool::Postgres, DatabasePool::Sqlite) |pool| {
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

                query
                    .query_builder
                    .build_query_as()
                    .fetch_all(pool)
                    .await
                    .map_err(DatabaseError::from)
                    .map_err(handle_database_error)?
            })
        };
        Ok(axum::Json(objects))
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
{
    async fn api_get(
        id: Path<Id>,
        database: State<AppState>,
        user: UserAuth,
    ) -> Result<axum::Json<Self>, StatusCode> {
        let group = user.0.group(database.0.clone(), None).await;
        let object = {
            database
                .get_one::<Self>(id.0, Some((&user.0, &group)))
                .await
                .map_err(handle_database_error)?
        };

        Ok(axum::Json(object))
    }
}

#[async_trait]
pub trait ApiCreate: ApiObject + FromJson
where
    Self: Sized + 'static,
    Self: Serialize,
    Self: Clone,
    Self: for<'a> IntoArguments<'a, sqlx::Sqlite>,
    Self: for<'a> IntoArguments<'a, sqlx::Postgres>,
{
    async fn api_create(
        database: State<AppState>,
        user: UserAuth,
        data: axum::extract::Json<Self::JsonFrom>,
    ) -> Result<axum::Json<Self>, StatusCode> {
        let database = database.0;
        let user = user.0;
        let mut data = data.0;
        let group = user.group(database.clone(), None).await;
        //in theory this is redundant, as database::insert checks it as well, but better safe than sorry
        if !Self::can_create(&user, &group) {
            return Err(StatusCode::UNAUTHORIZED);
        }

        let object = {
            debug!("running before create for /{}", Self::table_name());
            Self::before_api_create(database.clone(), &mut data, &user)
                .await
                .map_err(handle_database_error)?;
            let object = Self::from_json(&data, &user);
            let _ = database
                .insert(&object, Some((&user, &group)))
                .await
                .map_err(handle_database_error)?;

            debug!(
                "running after create for /{}/{}",
                Self::table_name(),
                object.id()
            );
            object
                .after_api_create(database, &mut data, &user)
                .await
                .map_err(handle_database_error)?;
            object
        };

        Ok(axum::Json(object))
    }

    #[allow(unused)]
    /// runs before the database entry creation
    async fn before_api_create(
        database: AppState,
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
        database: AppState,
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
{
    async fn api_update(
        id: Path<Id>,
        database: State<AppState>,
        user: UserAuth,
        data: axum::Json<Self::JsonUpdate>,
    ) -> Result<axum::Json<Self>, StatusCode> {
        let id = id.0;
        let database = database.0;
        let user = user.0;
        let mut data = data.0;
        let object = {
            let group = user.group(database.clone(), None).await;

            let object = database
                .get_one::<Self>(id, Some((&user, &group)))
                .await
                .map_err(handle_database_error)?;

            debug!(
                "running before update for /{}/{}",
                Self::table_name(),
                object.id()
            );
            object
                .before_api_update(database.clone(), &mut data, &user)
                .await
                .map_err(handle_database_error)?;

            let object = object.update_with_json(&data);

            let _ = database
                .update(&object, Some((&user, &group)))
                .await
                .map_err(handle_database_error)?;

            debug!(
                "running after update for /{}/{}",
                Self::table_name(),
                object.id()
            );
            object
                .after_api_update(database, &mut data, &user)
                .await
                .map_err(handle_database_error)?;
            object
        };

        Ok(axum::Json(object))
    }
    #[allow(unused)]
    /// runs before the database entry update
    async fn before_api_update(
        &self,
        database: AppState,
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
        database: AppState,
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
{
    async fn api_remove(
        id: Path<Id>,
        database: State<AppState>,
        user: UserAuth,
    ) -> Result<StatusCode, StatusCode> {
        let id = id.0;
        let database = database.0;
        let user = user.0;
        let group = user.group(database.clone(), None).await;

        let object = database
            .get_one::<Self>(id, Some((&user, &group)))
            .await
            .map_err(handle_database_error)?;

        debug!(
            "running before delete for /{}/{}",
            Self::table_name(),
            object.id()
        );
        object
            .before_api_delete(database.clone(), &user)
            .await
            .map_err(handle_database_error)?;

        let _ = database
            .remove(&object, Some((&user, &group)))
            .await
            .map_err(handle_database_error)?;

        debug!(
            "running after delete for /{}/{}",
            Self::table_name(),
            object.id()
        );
        object
            .after_api_delete(database.clone(), &user)
            .await
            .map_err(handle_database_error)?;

        Ok(StatusCode::NO_CONTENT)
    }

    #[allow(unused)]
    /// runs before the database entry deletion
    async fn before_api_delete(
        &self,
        database: AppState,
        user: &User,
    ) -> Result<(), DatabaseError> {
        Ok(())
    }
    #[allow(unused)]
    /// runs after the database entry deletion
    ///
    /// this returns a [`Result`], but there is no mechanism to undo the entry deletion. if this fails it should probably cause the program to panic
    async fn after_api_delete(&self, database: AppState, user: &User) -> Result<(), DatabaseError> {
        Ok(())
    }
}

pub trait ApiIcon: ApiObject
where
    Self: Sized + 'static,
    Self: for<'r> FromRow<'r, sqlx::sqlite::SqliteRow>,
    Self: for<'r> FromRow<'r, sqlx::postgres::PgRow>,
    Self: serde::Serialize,
    Self: Unpin,
{
    async fn upload_icon(
        state: State<AppState>,
        id: Path<Id>,
        user: UserAuth,
        headers: HeaderMap,
        mut multipart: Multipart,
    ) -> Result<impl IntoResponse, StatusCode> {
        let database = state.0;
        let id = id.0;
        let user = user.0;

        //check if the asset exists and the user has access to it
        let _ = database
            .get_one::<Self>(id, Some((&user, &user.group(database.clone(), None).await)))
            .await
            .map_err(|_| StatusCode::NOT_FOUND)?;

        let mut bytes = None;

        while let Some(field) = multipart.next_field().await.unwrap() {
            let filename = if let Some(filename) = field.file_name() {
                filename.to_string()
            } else {
                continue;
            };

            let field_bytes = if let Ok(bytes) = field.bytes().await {
                bytes
            } else {
                continue;
            };

            let extension = filename.split('.').last();

            if let Some(extension) = extension {
                let image_format = ImageFormat::from_extension(extension);
                if let Some(image_format) = image_format {
                    bytes = Some((field_bytes, image_format));
                    break;
                }
            }
        }

        let (bytes, image_format) = bytes.ok_or_else(|| StatusCode::BAD_REQUEST)?;

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
        }

        Ok(StatusCode::OK)
    }

    async fn get_icon(id: Path<Id>) -> Result<impl IntoResponse, StatusCode> {
        let mut path = icons_dir().join(Self::table_name());
        println!("{path:?}");
        let (path, is_gif) = if path.join(format!("{}.webp", id.to_string())).exists() {
            (path.join(format!("{}.webp", id.to_string())), false)
        } else if path.join(format!("{}.gif", id.to_string())).exists() {
            (path.join(format!("{}.gif", id.to_string())), true)
        } else {
            return Err(StatusCode::NOT_FOUND);
        };

        let file = tokio::fs::File::open(path)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
            .await?;

        let stream = ReaderStream::new(file);
        let body = axum::body::Body::from_stream(stream);

        let mime_type = if is_gif { "image/gif" } else { "image/webp" };

        let header = [(header::CONTENT_TYPE, mime_type)];

        Ok((header, body))
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

pub async fn user_register(
    database: State<AppState>,
    credentials: axum::extract::Json<Login>,
    query: axum::extract::Query<Vec<(String, String)>>,
) -> Result<axum::Json<User>, StatusCode> {
    let database = database.0;
    let credentials = credentials.0;
    let query = query.0;
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

    let mut token = None;
    for (parameter, value) in query {
        if parameter == "token" {
            token = Uuid::from_str(&value).ok();
            break;
        }
    }

    let mut can_continue = false;
    let invite = if let Some(token) = token {
        let invite: Result<InviteLink, _> = database.get_where("invite_token", token, None).await;
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

    if database
        .get_where::<User, _>("username", &credentials.username, None)
        .await
        .is_ok()
    {
        return Err(StatusCode::CONFLICT);
    }

    let user = database
        .create_user(&credentials.username, &credentials.password)
        .await
        .map_err(|err| {
            error!("{err}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    if let Some(invite) = invite {
        database.remove(&invite, None).await.map_err(|err| {
            error!("{err}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    }

    Ok(axum::Json(user))
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
    database: State<AppState>,
    session: WithSession,
) -> Result<StatusCode, StatusCode> {
    let database = database.0;
    let session = session.0;

    database
        .remove(&session, None)
        .await
        .map_err(handle_database_error)?;

    Ok(StatusCode::OK)
}

#[allow(clippy::unused_async)]
pub async fn user_info(user: UserAuth) -> Result<axum::Json<User>, StatusCode> {
    Ok(axum::Json(user.0))
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
    }

    Ok(axum::Json(ServerInfo {
        name: CONFIG.info.name.clone(),
        login_message: CONFIG.info.login_message.clone(),
        login_message_title: CONFIG.info.login_message_title.clone(),
        login_message_type: CONFIG.info.login_message_type.clone(),
        requires_invite: CONFIG.require_invite_to_register,
    }))
}

pub async fn get_username_valid(
    username: Path<String>,
    database: State<AppState>,
) -> impl IntoResponse {
    if database
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
    invite_link: Path<Uuid>,
    database: State<AppState>,
) -> impl IntoResponse {
    if database
        .get_where::<InviteLink, _>("invite_token", invite_link.0, None)
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
    hostname: Path<String>,
    database: State<AppState>,
    _: UserAuth,
) -> impl IntoResponse {
    if database
        .get_where::<World, _>("hostname", hostname.0, None)
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
    limits.max_image_width = Some(1024);
    limits.max_image_height = Some(1024);

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
