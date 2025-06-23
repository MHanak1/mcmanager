use crate::api::util::rejections;
use crate::api::{auth, filters};
use crate::database::objects::{DbObject, FromJson, UpdateJson, User};
use crate::database::types::Id;
use crate::database::{Database, DatabaseError};
use async_trait::async_trait;
use log::error;
use rusqlite::{Error, ErrorCode};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use rusqlite::fallible_iterator::FallibleIterator;
use tokio::sync::Mutex;
use warp::http::StatusCode;
use warp::{Filter, Rejection, Reply, reject};
use warp_rate_limit::{RateLimitConfig, RateLimitInfo};

pub type DbMutex = Arc<Mutex<Database>>;

pub trait ApiObject: DbObject {
    fn filters(
        db_mutex: DbMutex,
        rate_limit_config: RateLimitConfig,
    ) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone;
}

#[async_trait]
pub trait ApiList: ApiObject
where
    Self: Sized + 'static,
    Self: Serialize,
{
    //in theory the user filter should be done within the sql query, but for the sake of simplicity we do that when collecting the results
    async fn api_list(
        _rate_limit_info: RateLimitInfo,
        db_mutex: DbMutex,
        user: User,
        filters: Vec<(String, String)>,
    ) -> Result<impl warp::Reply, warp::Rejection> {
        let group = user.group(db_mutex.clone(), None).await;
        let objects = {
            let database = db_mutex.lock().await;
            database
                .list_filtered::<Self>(filters, Some((&user, &group)))
                .map_err(handle_database_error)?
        };
        Ok(warp::reply::with_status(
            warp::reply::json(&objects),
            StatusCode::OK,
        ))
    }

    fn list_filter(
        db_mutex: DbMutex,
        rate_limit_config: RateLimitConfig,
    ) -> impl Filter<Extract = (impl warp::Reply,), Error = warp::Rejection> + Clone + Send + Sync
    {
        warp::path("api")
            .and(warp_rate_limit::with_rate_limit(rate_limit_config))
            .and(warp::path(Self::table_name()))
            .and(warp::path::end())
            .and(warp::get())
            .and(filters::with_db(db_mutex.clone()))
            .and(filters::with_auth(db_mutex))
            .and(warp::query::<Vec<(String, String)>>())
            .and_then(Self::api_list)
    }
}
#[async_trait]
pub trait ApiGet: ApiObject
where
    Self: Sized + 'static,
    Self: Serialize,
{
    async fn api_get(
        _rate_limit_info: RateLimitInfo,
        id: String,
        db_mutex: DbMutex,
        user: User,
    ) -> Result<impl warp::Reply, warp::Rejection> {
        let id = Id::from_string(&id).map_err(|_| reject::custom(rejections::NotFound))?;
        let group = user.group(db_mutex.clone(), None).await;
        let object = {
            let database = db_mutex.lock().await;

            database
                .get_one::<Self>(id, Some((&user, &group)))
                .map_err(handle_database_error)?
        };

        Ok(warp::reply::with_status(
            warp::reply::json(&object),
            StatusCode::OK,
        ))
    }

    fn get_filter(
        db_mutex: DbMutex,
        rate_limit_config: RateLimitConfig,
    ) -> impl Filter<Extract = (impl warp::Reply,), Error = warp::Rejection> + Clone + Send + Sync
    {
        warp::path("api")
            .and(warp_rate_limit::with_rate_limit(rate_limit_config))
            .and(warp::path(Self::table_name()))
            .and(warp::path::param::<String>())
            .and(warp::path::end())
            .and(warp::get())
            .and(filters::with_db(db_mutex.clone()))
            .and(filters::with_auth(db_mutex))
            .and_then(Self::api_get)
    }
}

#[async_trait]
pub trait ApiCreate: ApiObject + FromJson
where
    Self: Sized + 'static,
    Self: Serialize,
{
    async fn api_create(
        _rate_limit_info: RateLimitInfo,
        db_mutex: DbMutex,
        user: User,
        data: Self::JsonFrom,
    ) -> Result<impl warp::Reply, warp::Rejection> {
        let mut data = data;
        let group = user.group(db_mutex.clone(), None).await;
        //in theory this is redundant, as database::insert checks it as well, but better safe than sorry
        if !Self::can_create(&user, &group) {
            return Err(reject::custom(rejections::Unauthorized));
        }

        let object = {
            Self::before_api_create(db_mutex.clone(), &mut data, &user)
                .await
                .map_err(handle_database_error)?;
            let object = Self::from_json(&data, &user);
            let _ = db_mutex
                .lock()
                .await
                .insert(&object, Some((&user, &group)))
                .map_err(handle_database_error)?;

            object
                .after_api_create(db_mutex, &mut data, &user)
                .await
                .map_err(handle_database_error)?;
            object
        };

        Ok(warp::reply::with_status(
            warp::reply::with_header(
                warp::reply::json(&object),
                warp::http::header::LOCATION,
                format!("api/{}/{}", Self::table_name(), object.get_id()),
            ),
            StatusCode::CREATED,
        ))
    }

    #[allow(unused)]
    /// runs before the database entry creation
    async fn before_api_create(
        database: DbMutex,
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
        database: DbMutex,
        json: &mut Self::JsonFrom,
        user: &User,
    ) -> Result<(), DatabaseError> {
        Ok(())
    }

    fn create_filter(
        db_mutex: DbMutex,
        rate_limit_config: RateLimitConfig,
    ) -> impl Filter<Extract = (impl warp::Reply,), Error = warp::Rejection> + Clone + Send + Sync
    {
        warp::path("api")
            .and(warp_rate_limit::with_rate_limit(rate_limit_config))
            .and(warp::path(Self::table_name()))
            .and(warp::path::end())
            .and(warp::post())
            .and(warp::body::content_length_limit(1024 * 32))
            .and(filters::with_db(db_mutex.clone()))
            .and(filters::with_auth(db_mutex))
            .and(warp::body::json())
            .and_then(Self::api_create)
    }
}

#[async_trait]
pub trait ApiUpdate: ApiObject + UpdateJson
where
    Self: Sized + 'static,
    Self: serde::Serialize,
{
    async fn api_update(
        _rate_limit_info: RateLimitInfo,
        id: String,
        db_mutex: DbMutex,
        user: User,
        data: Self::JsonUpdate,
    ) -> Result<impl warp::Reply, warp::Rejection> {
        let mut data = data;
        let object = {
            let id =
                Id::from_string(&id).map_err(|_| warp::reject::custom(rejections::NotFound))?;
            let group = user.group(db_mutex.clone(), None).await;

            let object = db_mutex
                .lock()
                .await
                .get_one::<Self>(id, Some((&user, &group)))
                .map_err(handle_database_error)?;

            object
                .before_api_update(db_mutex.clone(), &mut data, &user)
                .await
                .map_err(handle_database_error)?;

            let object = object.update_with_json(&data);

            let _ = db_mutex
                .lock()
                .await
                .update(&object, Some((&user, &group)))
                .map_err(handle_database_error)?;

            object
                .after_api_update(db_mutex, &mut data, &user)
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
        database: DbMutex,
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
        database: DbMutex,
        json: &mut Self::JsonUpdate,
        user: &User,
    ) -> Result<(), DatabaseError> {
        Ok(())
    }

    fn update_filter(
        db_mutex: DbMutex,
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
            .and(filters::with_db(db_mutex.clone()))
            .and(filters::with_auth(db_mutex))
            .and(warp::body::json())
            .and_then(Self::api_update)
    }
}

#[async_trait]
pub trait ApiRemove: ApiObject
where
    Self: Sized + 'static,
    Self: serde::Serialize,
{
    async fn api_remove(
        _rate_limit_info: RateLimitInfo,
        id: String,
        db_mutex: DbMutex,
        user: User,
    ) -> Result<impl warp::Reply, warp::Rejection> {
        let id = Id::from_string(&id).map_err(|_| reject::custom(rejections::NotFound))?;
        let group = user.group(db_mutex.clone(), None).await;

        {
            let database = db_mutex.lock().await;

            let object = database
                .get_one::<Self>(id, Some((&user, &group)))
                .map_err(handle_database_error)?;

            object
                .before_api_delete(&database, &user)
                .await
                .map_err(handle_database_error)?;

            let _ = database
                .remove(&object, Some((&user, &group)))
                .map_err(handle_database_error)?;

            object
                .after_api_delete(&database, &user)
                .await
                .map_err(handle_database_error)?;
        };

        Ok(warp::reply::with_status(
            warp::reply::json(&""),
            StatusCode::NO_CONTENT,
        ))
    }

    #[allow(unused)]
    /// runs before the database entry deletion
    async fn before_api_delete(
        &self,
        database: &Database,
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
        database: &Database,
        user: &User,
    ) -> Result<(), DatabaseError> {
        Ok(())
    }

    fn remove_filter(
        db_mutex: DbMutex,
        rate_limit_config: RateLimitConfig,
    ) -> impl Filter<Extract = (impl warp::Reply,), Error = warp::Rejection> + Clone + Send + Sync
    {
        warp::path("api")
            .and(warp_rate_limit::with_rate_limit(rate_limit_config))
            .and(warp::path(Self::table_name()))
            .and(warp::path::param::<String>())
            .and(warp::path::end())
            .and(warp::delete())
            .and(filters::with_db(db_mutex.clone()))
            .and(filters::with_auth(db_mutex))
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
        DatabaseError::SqliteError(err) => match err {
            Error::QueryReturnedNoRows => reject::custom(rejections::NotFound),
            Error::SqliteFailure(sql_err, ..) => match sql_err.code {
                ErrorCode::ConstraintViolation => reject::custom(rejections::Conflict),
                _ => reject::custom(rejections::InternalServerError::from(err.to_string())),
            },
            _ => reject::custom(rejections::InternalServerError::from(err.to_string())),
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

//this in theory could be transformed into ApiCreate implementation, but it would require a fair amount of changes, and for now it's not causing any problems
#[allow(clippy::unused_async)]
pub async fn user_auth(
    _rate_limit_info: RateLimitInfo,
    db_mutex: DbMutex,
    credentials: Login,
) -> Result<impl warp::Reply, warp::Rejection> {
    let session = {
        let database = db_mutex.lock().await;

        auth::try_user_auth(&credentials.username, &credentials.password, &database)
            .map_err(handle_database_error)?
    };

    Ok(warp::reply::with_status(
        warp::reply::with_header(
            warp::reply::json(&TokenReply {
                token: session.token.to_string(),
            }),
            "Set-Cookie",
            format!(
                "sessionToken={}; Path=/api; HttpOnly; Max-Age=1209600",
                session.token
            ),
        ),
        StatusCode::CREATED,
    ))
}

#[allow(clippy::unused_async)]
pub async fn user_info(user: User) -> Result<impl warp::Reply, warp::Rejection> {
    Ok(warp::reply::json(&user))
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
