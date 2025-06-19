use crate::api::util::rejections;
use crate::api::{auth, filters};
use crate::database::objects::{DbObject, FromJson, UpdateJson, User};
use crate::database::types::Id;
use crate::database::{Database, DatabaseError};
use log::error;
use rusqlite::{Error, ErrorCode};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use warp::http::StatusCode;
use warp::{Filter, Rejection, Reply, reject};
use warp_rate_limit::{RateLimitConfig, RateLimitInfo};

pub trait ApiObject: DbObject {
    fn filters(
        db_mutex: Arc<Mutex<Database>>,
        rate_limit_config: RateLimitConfig,
    ) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone;
}

pub trait ApiList: ApiObject
where
    Self: Sized,
    Self: Serialize,
{
    //in theory the user filter should be done within the sql query, but for the sake of simplicity we do that when collecting the results
    fn api_list(
        _rate_limit_info: RateLimitInfo,
        db_mutex: Arc<Mutex<Database>>,
        user: User,
        filters: Vec<(String, String)>,
    ) -> Result<impl Reply, warp::Rejection> {
        let objects = {
            let database = db_mutex.lock().map_err(|err| {
                reject::custom(rejections::InternalServerError::from(err.to_string()))
            })?;
            database
                .list_filtered::<Self>(filters, Some(&user))
                .map_err(handle_database_error)?
        };
        Ok(warp::reply::with_status(
            warp::reply::json(&objects),
            StatusCode::OK,
        ))
    }

    fn list_filter(
        db_mutex: Arc<Mutex<Database>>,
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
            .and_then(|rate_limit_info, db_mutex, user, filters| async move {
                Self::api_list(rate_limit_info, db_mutex, user, filters)
            })
    }
}
pub trait ApiGet: ApiObject
where
    Self: Sized,
    Self: Serialize,
{
    fn api_get(
        _rate_limit_info: RateLimitInfo,
        id: String,
        db_mutex: Arc<Mutex<Database>>,
        user: User,
    ) -> Result<impl warp::Reply, warp::Rejection> {
        let id = Id::from_string(&id).map_err(|_| reject::custom(rejections::NotFound))?;
        let object = {
            let database = db_mutex.lock().map_err(|err| {
                reject::custom(rejections::InternalServerError::from(err.to_string()))
            })?;

            database
                .get_one::<Self>(id, Some(&user))
                .map_err(handle_database_error)?
        };

        Ok(warp::reply::with_status(
            warp::reply::json(&object),
            StatusCode::OK,
        ))
    }

    fn get_filter(
        db_mutex: Arc<Mutex<Database>>,
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
            .and_then(|rate_limit_info, id, db_mutex, user| async move {
                Self::api_get(rate_limit_info, id, db_mutex, user)
            })
    }
}

pub trait ApiCreate: ApiObject + FromJson
where
    Self: Sized,
    Self: Serialize,
{
    fn api_create(
        _rate_limit_info: RateLimitInfo,
        db_mutex: Arc<Mutex<Database>>,
        user: User,
        data: Self::JsonFrom,
    ) -> Result<impl Reply, warp::Rejection> {
        let mut data = data;
        //in theory this is redundant, as database::insert checks it as well, but better safe than sorry
        if !Self::can_create(&user) {
            return Err(reject::custom(rejections::Unauthorized));
        }

        let object = {
            let database = db_mutex.lock().map_err(|err| {
                reject::custom(rejections::InternalServerError::from(err.to_string()))
            })?;

            Self::before_api_create(&database, &mut data).map_err(handle_database_error)?;
            let object = Self::from_json(&data, &user);
            let _ = database
                .insert(&object, Some(&user))
                .map_err(handle_database_error)?;

            object
                .after_api_create(&database, &mut data)
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
    fn before_api_create(
        database: &Database,
        json: &mut Self::JsonFrom,
    ) -> Result<(), DatabaseError> {
        Ok(())
    }
    #[allow(unused)]
    /// runs after the database entry creation
    ///
    /// this returns a [`Result`], but there is no mechanism to undo the entry creation. if this fails it should probably cause the program to panic
    fn after_api_create(
        &self,
        database: &Database,
        json: &mut Self::JsonFrom,
    ) -> Result<(), DatabaseError> {
        Ok(())
    }

    fn create_filter(
        db_mutex: Arc<Mutex<Database>>,
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
            .and_then(|rate_limit_info, db_mutex, user, data| async move {
                Self::api_create(rate_limit_info, db_mutex, user, data)
            })
    }
}

pub trait ApiUpdate: ApiObject + UpdateJson
where
    Self: Sized,
    Self: serde::Serialize,
{
    fn api_update(
        _rate_limit_info: RateLimitInfo,
        db_mutex: Arc<Mutex<Database>>,
        id: String,
        user: User,
        data: Self::JsonUpdate,
    ) -> Result<impl Reply, warp::Rejection> {
        let mut data = data;
        let object = {
            let database = db_mutex.lock().map_err(|err| {
                reject::custom(rejections::InternalServerError::from(err.to_string()))
            })?;

            let id =
                Id::from_string(&id).map_err(|_| warp::reject::custom(rejections::NotFound))?;

            let object = database
                .get_one::<Self>(id, Some(&user))
                .map_err(handle_database_error)?;

            object
                .before_api_update(&database, &mut data)
                .map_err(handle_database_error)?;

            let object = object.update_with_json(&data);

            let _ = database
                .update(&object, Some(&user))
                .map_err(handle_database_error)?;

            object
                .after_api_update(&database, &mut data)
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
    fn before_api_update(
        &self,
        database: &Database,
        json: &mut Self::JsonUpdate,
    ) -> Result<(), DatabaseError> {
        Ok(())
    }
    #[allow(unused)]
    /// runs after the database entry update
    ///
    /// this returns a [`Result`], but there is no mechanism to undo the entry update. if this fails it should probably cause the program to panic
    fn after_api_update(
        &self,
        database: &Database,
        json: &mut Self::JsonUpdate,
    ) -> Result<(), DatabaseError> {
        Ok(())
    }

    fn update_filter(
        db_mutex: Arc<Mutex<Database>>,
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
            .and_then(|rate_limit_info, id, db_mutex, user, data| async move {
                Self::api_update(rate_limit_info, db_mutex, id, user, data)
            })
    }
}

pub trait ApiRemove: ApiObject
where
    Self: Sized,
    Self: serde::Serialize,
{
    fn api_remove(
        _rate_limit_info: RateLimitInfo,
        id: String,
        db_mutex: Arc<Mutex<Database>>,
        user: User,
    ) -> Result<impl Reply, warp::Rejection> {
        let id = Id::from_string(&id).map_err(|_| reject::custom(rejections::NotFound))?;

        {
            let database = db_mutex.lock().map_err(|err| {
                reject::custom(rejections::InternalServerError::from(err.to_string()))
            })?;

            let object = database
                .get_one::<Self>(id, Some(&user))
                .map_err(handle_database_error)?;

            object
                .before_api_delete(&database)
                .map_err(handle_database_error)?;

            let _ = database
                .remove(&object, Some(&user))
                .map_err(handle_database_error)?;

            object
                .after_api_delete(&database)
                .map_err(handle_database_error)?;
        };

        Ok(warp::reply::with_status(
            warp::reply::json(&""),
            StatusCode::NO_CONTENT,
        ))
    }

    #[allow(unused)]
    /// runs before the database entry deletion
    fn before_api_delete(&self, database: &Database) -> Result<(), DatabaseError> {
        Ok(())
    }
    #[allow(unused)]
    /// runs after the database entry deletion
    ///
    /// this returns a [`Result`], but there is no mechanism to undo the entry deletion. if this fails it should probably cause the program to panic
    fn after_api_delete(&self, database: &Database) -> Result<(), DatabaseError> {
        Ok(())
    }

    fn remove_filter(
        db_mutex: Arc<Mutex<Database>>,
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
            .and_then(|rate_limit_info, id, db_mutex, user| async move {
                Self::api_remove(rate_limit_info, id, db_mutex, user)
            })
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
    db_mutex: Arc<Mutex<Database>>,
    credentials: Login,
) -> Result<impl warp::Reply, warp::Rejection> {
    let session = {
        let database = db_mutex.lock().map_err(|err| {
            reject::custom(rejections::InternalServerError::from(err.to_string()))
        })?;

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
