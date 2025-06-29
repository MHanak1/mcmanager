use crate::api::util::rejections;
use crate::api::util::rejections::BadRequest;
use crate::api::{auth, filters};
use crate::config::CONFIG;
use crate::database::DatabaseError::SqlxError;
use crate::database::objects::{DbObject, FromJson, InviteLink, UpdateJson, User, World};
use crate::database::types::Id;
pub(crate) use crate::database::{Database, DatabaseError};
use crate::database::{DatabasePool, QueryBuilder, ValueType, WhereOperand};
use crate::util::base64::base64_decode;
use async_trait::async_trait;
use chrono::DateTime;
use futures::StreamExt;
use log::error;
use serde::{Deserialize, Serialize};
use sqlx::{Encode, FromRow, IntoArguments, Type};
use std::io::Read;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;
use warp::http::StatusCode;
use warp::{Filter, Rejection, Reply, reject};
use warp_rate_limit::{RateLimitConfig, RateLimitInfo};
use crate::execute_on_enum;

pub trait ApiObject: DbObject {
    fn filters(
        database: Arc<Database>,
        rate_limit_config: RateLimitConfig,
    ) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone;
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
        _rate_limit_info: RateLimitInfo,
        database: Arc<Database>,
        user: User,
        filters: Vec<(String, String)>,
    ) -> Result<impl warp::Reply, warp::Rejection> {
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
        Ok(warp::reply::with_status(
            warp::reply::json(&objects),
            StatusCode::OK,
        ))
    }

    fn list_filter(
        database: Arc<Database>,
        rate_limit_config: RateLimitConfig,
    ) -> impl Filter<Extract = (impl warp::Reply,), Error = warp::Rejection> + Clone + Send + Sync
    {
        warp::path("api")
            .and(warp_rate_limit::with_rate_limit(rate_limit_config))
            .and(warp::path(Self::table_name()))
            .and(warp::path::end())
            .and(warp::get())
            .and(filters::with_db(database.clone()))
            .and(filters::with_auth(database))
            .and(warp::query::<Vec<(String, String)>>())
            .and_then(Self::api_list)
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
        _rate_limit_info: RateLimitInfo,
        id: String,
        database: Arc<Database>,
        user: User,
    ) -> Result<impl warp::Reply, warp::Rejection> {
        let id = Id::from_string(&id).map_err(|_| reject::custom(rejections::NotFound))?;
        let group = user.group(database.clone(), None).await;
        let object = {
            database
                .get_one::<Self>(id, Some((&user, &group)))
                .await
                .map_err(handle_database_error)?
        };

        Ok(warp::reply::with_status(
            warp::reply::json(&object),
            StatusCode::OK,
        ))
    }

    fn get_filter(
        database: Arc<Database>,
        rate_limit_config: RateLimitConfig,
    ) -> impl Filter<Extract = (impl warp::Reply,), Error = warp::Rejection> + Clone + Send + Sync
    {
        warp::path("api")
            .and(warp_rate_limit::with_rate_limit(rate_limit_config))
            .and(warp::path(Self::table_name()))
            .and(warp::path::param::<String>())
            .and(warp::path::end())
            .and(warp::get())
            .and(filters::with_db(database.clone()))
            .and(filters::with_auth(database))
            .and_then(Self::api_get)
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
        _rate_limit_info: RateLimitInfo,
        database: Arc<Database>,
        user: User,
        data: Self::JsonFrom,
    ) -> Result<impl warp::Reply, warp::Rejection> {
        let mut data = data;
        let group = user.group(database.clone(), None).await;
        //in theory this is redundant, as database::insert checks it as well, but better safe than sorry
        if !Self::can_create(&user, &group) {
            return Err(reject::custom(rejections::Unauthorized));
        }

        let object = {
            Self::before_api_create(database.clone(), &mut data, &user)
                .await
                .map_err(handle_database_error)?;
            let object = Self::from_json(&data, &user);
            let _ = database
                .insert(&object, Some((&user, &group)))
                .await
                .map_err(handle_database_error)?;

            object
                .after_api_create(database, &mut data, &user)
                .await
                .map_err(handle_database_error)?;
            object
        };

        Ok(warp::reply::with_status(
            warp::reply::with_header(
                warp::reply::json(&object),
                warp::http::header::LOCATION,
                format!("api/{}/{}", Self::table_name(), object.id()),
            ),
            StatusCode::CREATED,
        ))
    }

    #[allow(unused)]
    /// runs before the database entry creation
    async fn before_api_create(
        database: Arc<Database>,
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
        database: Arc<Database>,
        json: &mut Self::JsonFrom,
        user: &User,
    ) -> Result<(), DatabaseError> {
        Ok(())
    }

    fn create_filter(
        database: Arc<Database>,
        rate_limit_config: RateLimitConfig,
    ) -> impl Filter<Extract = (impl warp::Reply,), Error = warp::Rejection> + Clone + Send + Sync
    {
        warp::path("api")
            .and(warp_rate_limit::with_rate_limit(rate_limit_config))
            .and(warp::path(Self::table_name()))
            .and(warp::path::end())
            .and(warp::post())
            .and(warp::body::content_length_limit(1024 * 32))
            .and(filters::with_db(database.clone()))
            .and(filters::with_auth(database))
            .and(warp::body::json())
            .and_then(Self::api_create)
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
        _rate_limit_info: RateLimitInfo,
        id: String,
        database: Arc<Database>,
        user: User,
        data: Self::JsonUpdate,
    ) -> Result<impl warp::Reply, warp::Rejection> {
        let mut data = data;
        let object = {
            let id =
                Id::from_string(&id).map_err(|_| warp::reject::custom(rejections::NotFound))?;
            let group = user.group(database.clone(), None).await;

            let object = database
                .get_one::<Self>(id, Some((&user, &group)))
                .await
                .map_err(handle_database_error)?;

            object
                .before_api_update(database.clone(), &mut data, &user)
                .await
                .map_err(handle_database_error)?;

            let object = object.update_with_json(&data);

            let _ = database
                .update(&object, Some((&user, &group)))
                .await
                .map_err(handle_database_error)?;

            object
                .after_api_update(database, &mut data, &user)
                .await
                .map_err(handle_database_error)?;
            object
        };

        Ok(warp::reply::with_status(
            warp::reply::json(&object),
            StatusCode::OK,
        ))
    }
    #[allow(unused)]
    /// runs before the database entry update
    async fn before_api_update(
        &self,
        database: Arc<Database>,
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
        database: Arc<Database>,
        json: &mut Self::JsonUpdate,
        user: &User,
    ) -> Result<(), DatabaseError> {
        Ok(())
    }

    fn update_filter(
        database: Arc<Database>,
        rate_limit_config: RateLimitConfig,
    ) -> impl Filter<Extract = (impl warp::Reply,), Error = warp::Rejection> + Clone + Send + Sync
    {
        warp::path("api")
            .and(warp_rate_limit::with_rate_limit(rate_limit_config))
            .and(warp::path(Self::table_name()))
            .and(warp::path::param::<String>())
            .and(warp::path::end())
            .and(warp::put())
            .and(warp::body::content_length_limit(1024 * 32))
            .and(filters::with_db(database.clone()))
            .and(filters::with_auth(database))
            .and(warp::body::json())
            .and_then(Self::api_update)
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
        _rate_limit_info: RateLimitInfo,
        id: String,
        database: Arc<Database>,
        user: User,
    ) -> Result<impl warp::Reply, warp::Rejection> {
        let id = Id::from_string(&id).map_err(|_| reject::custom(rejections::NotFound))?;
        let group = user.group(database.clone(), None).await;

        let object = database
            .get_one::<Self>(id, Some((&user, &group)))
            .await
            .map_err(handle_database_error)?;

        object
            .before_api_delete(database.clone(), &user)
            .await
            .map_err(handle_database_error)?;

        let _ = database
            .remove(&object, Some((&user, &group)))
            .await
            .map_err(handle_database_error)?;

        object
            .after_api_delete(database.clone(), &user)
            .await
            .map_err(handle_database_error)?;

        Ok(warp::reply::with_status(
            warp::reply::json(&""),
            StatusCode::NO_CONTENT,
        ))
    }

    #[allow(unused)]
    /// runs before the database entry deletion
    async fn before_api_delete(
        &self,
        database: Arc<Database>,
        user: &User,
    ) -> Result<(), DatabaseError> {
        Ok(())
    }
    #[allow(unused)]
    /// runs after the database entry deletion
    ///
    /// this returns a [`Result`], but there is no mechanism to undo the entry deletion. if this fails it should probably cause the program to panic
    async fn after_api_delete(
        &self,
        database: Arc<Database>,
        user: &User,
    ) -> Result<(), DatabaseError> {
        Ok(())
    }

    fn remove_filter(
        database: Arc<Database>,
        rate_limit_config: RateLimitConfig,
    ) -> impl Filter<Extract = (impl warp::Reply,), Error = warp::Rejection> + Clone + Send + Sync
    {
        warp::path("api")
            .and(warp_rate_limit::with_rate_limit(rate_limit_config))
            .and(warp::path(Self::table_name()))
            .and(warp::path::param::<String>())
            .and(warp::path::end())
            .and(warp::delete())
            .and(filters::with_db(database.clone()))
            .and(filters::with_auth(database))
            .and_then(Self::api_remove)
    }
}

pub(crate) fn handle_database_error(err: DatabaseError) -> warp::Rejection {
    error!("{err}");
    match err {
        DatabaseError::Unauthorized => reject::custom(rejections::Unauthorized),
        DatabaseError::NotFound => reject::custom(rejections::NotFound),
        DatabaseError::InternalServerError(error) => {
            reject::custom(rejections::InternalServerError { error })
        }
        DatabaseError::SqlxError(error) => match error {
            sqlx::Error::RowNotFound => reject::custom(rejections::NotFound),
            _ => reject::custom(rejections::InternalServerError {
                error: error.to_string(),
            }),
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
    _rate_limit_info: RateLimitInfo,
    database: Arc<Database>,
    credentials: Login,
    query: Vec<(String, String)>,
) -> Result<impl warp::Reply, warp::Rejection> {
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
        return Err(warp::reject::custom(rejections::BadRequest));
    }
    for char in credentials.username.chars() {
        if !ALLOWED_USERNAME_CHARS.contains(char) {
            return Err(warp::reject::custom(rejections::BadRequest));
        }
    }
    for char in credentials.password.chars() {
        if !ALLOWED_PASSWORD_CHARS.contains(char) {
            return Err(warp::reject::custom(rejections::BadRequest));
        }
    }

    let mut token = None;
    for (parameter, value) in query {
        if parameter == "token" {
            token =
                Some(Uuid::from_str(&value).map_err(|_| reject::custom(rejections::BadRequest))?);
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
        return Err(reject::custom(rejections::Unauthorized));
    }

    let user = database
        .create_user(&credentials.username, &credentials.password)
        .await
        .map_err(|err| warp::reject::custom(rejections::InternalServerError::from(err)))?;
    if let Some(invite) = invite {
        database
            .remove(&invite, None)
            .await
            .map_err(|err| warp::reject::custom(rejections::InternalServerError::from(err)))?;
    }

    Ok(warp::reply::with_status(
        warp::reply::json(&user),
        StatusCode::CREATED,
    ))
}

//this in theory could be transformed into ApiCreate implementation, but it would require a fair amount of changes, and for now it's not causing any problems
#[allow(clippy::unused_async)]
pub async fn user_auth(
    _rate_limit_info: RateLimitInfo,
    database: Arc<Database>,
    credentials: Login,
) -> Result<impl warp::Reply, warp::Rejection> {
    let session = {
        let database = database;

        auth::try_user_auth(&credentials.username, &credentials.password, database)
            .await
            .map_err(handle_database_error)?
    };

    Ok(warp::reply::with_status(
        warp::reply::with_header(
            warp::reply::json(&TokenReply {
                token: session.token.as_simple().to_string(),
            }),
            "Set-Cookie",
            format!(
                "sessionToken={}; Path=/api; HttpOnly; Max-Age=1209600",
                session.token.as_simple().to_string()
            ),
        ),
        StatusCode::CREATED,
    ))
}

#[allow(clippy::unused_async)]
pub async fn user_info(
    _rate_limit_info: RateLimitInfo,
    user: User,
) -> Result<impl warp::Reply, warp::Rejection> {
    Ok(warp::reply::json(&user))
}

pub async fn check_free<
    T: for<'r> Encode<'r, sqlx::Sqlite>
        + Type<sqlx::Sqlite>
        + for<'r> Encode<'r, sqlx::Postgres>
        + Type<sqlx::Postgres>
        + Clone
        + Send
        + Sync,
>(
    _rate_limit_info: RateLimitInfo,
    field: String,
    value: T,
    database: Arc<Database>,
) -> Result<impl warp::Reply, warp::Rejection> {
    #[derive(Serialize)]
    struct Used {
        taken: bool,
    }
    match field.trim() {
        "username" => match database.get_where::<User, _>("username", value, None).await {
            Ok(user) => Ok(warp::reply::with_status(
                warp::reply::json(&Used { taken: false }),
                StatusCode::OK,
            )),
            Err(err) => Ok(warp::reply::with_status(
                warp::reply::json(&Used { taken: true }),
                StatusCode::OK,
            )),
        },
        "hostname" => {
            match database
                .get_where::<World, _>("hostname", value, None)
                .await
            {
                Ok(world) => Ok(warp::reply::with_status(
                    warp::reply::json(&Used { taken: false }),
                    StatusCode::OK,
                )),
                Err(err) => Ok(warp::reply::with_status(
                    warp::reply::json(&Used { taken: true }),
                    StatusCode::OK,
                )),
            }
        }
        _ => Err(warp::reject::custom(rejections::MethodNotAllowed)),
    }
}

#[allow(clippy::unused_async)]
pub async fn handle_rejection(
    rejection: warp::Rejection,
) -> Result<impl warp::Reply, std::convert::Infallible> {
    let code;
    let message;

    #[derive(Serialize)]
    struct ErrorMessage {
        code: u16,
        message: String,
    }

    if rejection.find::<rejections::NotFound>().is_some() {
        code = StatusCode::NOT_FOUND;
        message = "not found";
    } else if let Some(error) = rejection.find::<rejections::InternalServerError>() {
        error!("{}", error.error);
        code = StatusCode::INTERNAL_SERVER_ERROR;
        message = "internal server error";
    } else if rejection.find::<rejections::InvalidBearerToken>().is_some() {
        code = StatusCode::UNAUTHORIZED;
        message = "invalid brearer token";
    } else if rejection.find::<rejections::Unauthorized>().is_some() {
        code = StatusCode::UNAUTHORIZED;
        message = "unauthorized";
    } else if rejection.find::<rejections::BadRequest>().is_some() {
        code = StatusCode::BAD_REQUEST;
        message = "bad request";
    } else if rejection.find::<rejections::NotImplemented>().is_some() {
        code = StatusCode::NOT_IMPLEMENTED;
        message = "not implemented";
    } else if rejection.find::<rejections::Conflict>().is_some() {
        code = StatusCode::CONFLICT;
        message = "conflict";
    } else if rejection.find::<warp::reject::InvalidQuery>().is_some() {
        code = StatusCode::BAD_REQUEST;
        message = "invalid query";
    } else if rejection.find::<warp::reject::InvalidHeader>().is_some() {
        code = StatusCode::BAD_REQUEST;
        message = "invalid header";
    } else if rejection.find::<warp::reject::LengthRequired>().is_some() {
        code = StatusCode::LENGTH_REQUIRED;
        message = "length required";
    } else if rejection.find::<warp::reject::PayloadTooLarge>().is_some() {
        code = StatusCode::PAYLOAD_TOO_LARGE;
        message = "payload too large";
    } else if rejection
        .find::<warp::reject::UnsupportedMediaType>()
        .is_some()
    {
        code = StatusCode::UNSUPPORTED_MEDIA_TYPE;
        message = "unsupported media type";
    } else if let Some(rejection) = rejection.find::<warp_rate_limit::RateLimitRejection>() {
        let info = warp_rate_limit::get_rate_limit_info(rejection);

        let mut response = warp::reply::with_status(
            warp::reply::json(&ErrorMessage {
                code: StatusCode::TOO_MANY_REQUESTS.as_u16(),
                message: "too many requests".to_string(),
            }),
            StatusCode::TOO_MANY_REQUESTS,
        )
        .into_response();

        let _ = warp_rate_limit::add_rate_limit_headers(response.headers_mut(), &info);

        return Ok(response);
    } else if rejection.find::<warp::reject::MethodNotAllowed>().is_some() {
        code = StatusCode::METHOD_NOT_ALLOWED;
        message = "method not allowed";
    } else {
        error!("unhandled rejection: {rejection:?}");
        code = StatusCode::INTERNAL_SERVER_ERROR;
        message = "unhandled rejection";
    }

    let json = warp::reply::json(&ErrorMessage {
        code: code.as_u16(),
        message: message.into(),
    });

    Ok(warp::reply::with_status(json, code).into_response())
}
