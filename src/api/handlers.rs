use crate::api::util::rejections;
use crate::api::{auth, filters};
use crate::database::Database;
use crate::database::objects::{DbObject, FromJson, UpdateJson, User};
use crate::database::types::Id;
use log::error;
use rusqlite::Error;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use warp::Filter;
use warp::http::StatusCode;

pub trait ApiList: DbObject
where
    Self: Sized,
    Self: Serialize,
{
    //in theory the user filter should be done within the sql query, but for the sake of simplicity we do that when collecting the results
    fn api_list(
        db_mutex: Arc<Mutex<Database>>,
        user: User,
        filters: HashMap<String, String>,
    ) -> Result<impl warp::Reply, warp::Rejection> {
        db_mutex.lock().map_or_else(
            |err| {
                Err(warp::reject::custom(rejections::InternalServerError::from(
                    err.to_string(),
                )))
            },
            |database| {
                match database.list_filtered::<Self>(filters, Some(&user)) {
                    Ok(objects) => Ok(warp::reply::with_status(
                        warp::reply::json(&objects),
                        StatusCode::OK,
                    )),
                    Err(err) => {
                        match err {
                            //if the user does not have access to anything, instead of erroring out return an empty array
                            Error::QueryReturnedNoRows => Ok(warp::reply::with_status(
                                warp::reply::json::<Vec<&str>>(&vec![]),
                                StatusCode::OK,
                            )),
                            _ => {
                                error!("{err:?}");
                                Err(warp::reject::custom(rejections::InternalServerError::from(
                                    err.to_string(),
                                )))
                            }
                        }
                    }
                }
            },
        )
    }

    fn list_filter(
        db_mutex: Arc<Mutex<Database>>,
    ) -> impl Filter<Extract = (impl warp::Reply,), Error = warp::Rejection> + Clone {
        warp::get()
            .and(warp::path("api"))
            .and(warp::path(Self::table_name()))
            .and(warp::path::end())
            .and(filters::with_db(db_mutex.clone()))
            .and(filters::with_auth(db_mutex))
            .and(warp::query::<HashMap<String, String>>())
            .and_then(
                |db_mutex, user, filters| async move { Self::api_list(db_mutex, user, filters) },
            )
    }
}
pub trait ApiGet: DbObject
where
    Self: Sized,
    Self: Serialize,
{
    fn api_get(
        id: String,
        db_mutex: Arc<Mutex<Database>>,
        user: User,
    ) -> Result<impl warp::Reply, warp::Rejection> {
        if let Ok(id) = Id::from_string(&id) {
            db_mutex.lock().map_or_else(
                |_| Err(warp::reject::not_found()),
                |database| match database.get_one::<Self>(id, Some(&user)) {
                    Ok(object) => Ok(warp::reply::with_status(
                        warp::reply::json(&object),
                        StatusCode::OK,
                    )),
                    Err(err) => match err {
                        Error::QueryReturnedNoRows => {
                            Err(warp::reject::custom(rejections::NotFound))
                        }
                        _ => {
                            error!("{err:?}");
                            Err(warp::reject::custom(rejections::InternalServerError::from(
                                err.to_string(),
                            )))
                        }
                    },
                },
            )
        } else {
            Err(warp::reject::custom(rejections::NotFound))
        }
    }

    fn get_filter(
        db_mutex: Arc<Mutex<Database>>,
    ) -> impl Filter<Extract = (impl warp::Reply,), Error = warp::Rejection> + Clone {
        warp::get()
            .and(warp::path("api"))
            .and(warp::path(Self::table_name()))
            .and(warp::path::param::<String>())
            .and(warp::path::end())
            .and(filters::with_db(db_mutex.clone()))
            .and(filters::with_auth(db_mutex))
            .and_then(|id, db_mutex, user| async move { Self::api_get(id, db_mutex, user) })
    }
}

pub trait ApiCreate: DbObject + FromJson
where
    Self: Sized,
    Self: Serialize,
{
    fn api_create(
        db_mutex: Arc<Mutex<Database>>,
        user: User,
        data: Self::JsonFrom,
    ) -> Result<impl warp::Reply, warp::Rejection> {
        if Self::can_create(&user) {
            db_mutex.lock().map_or_else(
                |err| {
                    Err(warp::reject::custom(rejections::InternalServerError::from(
                        err.to_string(),
                    )))
                },
                |database| {
                    if !Self::can_create(&user) {
                        return Err(warp::reject::custom(rejections::Unauthorized));
                    }

                    let object = Self::from_json(data, user.clone());

                    match database.insert(&object, Some(&user)) {
                        Ok(_) => Ok(warp::reply::with_status(
                            warp::reply::json(&object),
                            StatusCode::CREATED,
                        )),
                        Err(err) => {
                            error!("{err:?}");
                            Ok(warp::reply::with_status(
                                warp::reply::json(&"internal server error"),
                                StatusCode::INTERNAL_SERVER_ERROR,
                            ))
                        }
                    }
                },
            )
        } else {
            Ok(warp::reply::with_status(
                warp::reply::json(&"Unauthorized"),
                StatusCode::UNAUTHORIZED,
            ))
        }
    }

    fn create_filter(
        db_mutex: Arc<Mutex<Database>>,
    ) -> impl Filter<Extract = (impl warp::Reply,), Error = warp::Rejection> + Clone {
        warp::post()
            .and(warp::body::content_length_limit(1024 * 32))
            .and(warp::path("api"))
            .and(warp::path(Self::table_name()))
            .and(warp::path::end())
            .and(filters::with_db(db_mutex.clone()))
            .and(filters::with_auth(db_mutex))
            .and(warp::body::json())
            .and_then(|db_mutex, user, data| async move { Self::api_create(db_mutex, user, data) })
    }
}

pub trait ApiUpdate: ApiCreate + UpdateJson
where
    Self: Sized,
    Self: serde::Serialize,
{
    fn api_update(
        db_mutex: Arc<Mutex<Database>>,
        id: String,
        user: User,
        data: Self::JsonFrom,
    ) -> Result<impl warp::Reply, warp::Rejection> {
        db_mutex.lock().map_or_else(
            |err| {
                Err(warp::reject::custom(rejections::InternalServerError::from(
                    err.to_string(),
                )))
            },
            |database| {
                if let Ok(id) = Id::from_string(&id) {
                    database.get_one(id, Some(&user)).map_or_else(
                        |_| Err(warp::reject::custom(rejections::NotFound)),
                        |object: Self| {
                            let object = object.update_with_json(data);
                            match database.update(&object, Some(&user)) {
                                Ok(_) => Ok(warp::reply::with_status(
                                    warp::reply::json(&object),
                                    StatusCode::CREATED,
                                )),
                                Err(err) => {
                                    error!("{err:?}");
                                    Err(warp::reject::custom(rejections::Unauthorized))
                                }
                            }
                        },
                    )
                } else {
                    Err(warp::reject::custom(rejections::NotFound))
                }
            },
        )
    }

    fn update_filter(
        db_mutex: Arc<Mutex<Database>>,
    ) -> impl Filter<Extract = (impl warp::Reply,), Error = warp::Rejection> + Clone {
        warp::put()
            .and(warp::body::content_length_limit(1024 * 32))
            .and(warp::path("api"))
            .and(warp::path(Self::table_name()))
            .and(warp::path::param::<String>())
            .and(warp::path::end())
            .and(filters::with_db(db_mutex.clone()))
            .and(filters::with_auth(db_mutex))
            .and(warp::body::json())
            .and_then(|id, db_mutex, user, data| async move {
                Self::api_update(db_mutex, id, user, data)
            })
    }
}

pub trait ApiRemove: DbObject
where
    Self: Sized,
    Self: serde::Serialize,
{
    fn api_remove(
        id: String,
        db_mutex: Arc<Mutex<Database>>,
        user: User,
    ) -> Result<impl warp::Reply, warp::Rejection> {
        if let Ok(id) = Id::from_string(&id) {
            db_mutex.lock().map_or_else(
                |_| Err(warp::reject::not_found()),
                |database| match database.get_one::<Self>(id, Some(&user)) {
                    Ok(object) => match database.remove(&object, Some(&user)) {
                        Ok(_) => Ok(warp::reply::with_status(
                            warp::reply::json(&""),
                            StatusCode::OK,
                        )),
                        Err(err) => match err {
                            Error::QueryReturnedNoRows => {
                                Err(warp::reject::custom(rejections::NotFound))
                            }
                            _ => {
                                error!("{err:?}");
                                Err(warp::reject::custom(rejections::InternalServerError::from(
                                    err.to_string(),
                                )))
                            }
                        },
                    },
                    Err(err) => match err {
                        Error::QueryReturnedNoRows => {
                            Err(warp::reject::custom(rejections::NotFound))
                        }
                        _ => {
                            error!("{err:?}");
                            Err(warp::reject::custom(rejections::InternalServerError::from(
                                err.to_string(),
                            )))
                        }
                    },
                },
            )
        } else {
            Err(warp::reject::custom(rejections::NotFound))
        }
    }

    fn remove_filter(
        db_mutex: Arc<Mutex<Database>>,
    ) -> impl Filter<Extract = (impl warp::Reply,), Error = warp::Rejection> + Clone {
        warp::delete()
            .and(warp::path("api"))
            .and(warp::path(Self::table_name()))
            .and(warp::path::param::<String>())
            .and(warp::path::end())
            .and(filters::with_db(db_mutex.clone()))
            .and(filters::with_auth(db_mutex))
            .and_then(|id, db_mutex, user| async move { Self::api_remove(id, db_mutex, user) })
    }
}

mod json_fields {
    use serde::Deserialize;

    #[derive(Debug, Clone, Deserialize)]
    pub struct Login {
        pub username: String,
        pub password: String,
    }
}

#[derive(Serialize)]
struct TokenReply {
    token: String,
}

//this in theory could be transformed into ApiCreate implementation, but it would require a fair amount of changes, and for now it's not causing any problems
#[allow(clippy::unused_async)]
pub async fn user_auth(
    db_mutex: Arc<Mutex<Database>>,
    credentials: json_fields::Login,
) -> Result<impl warp::Reply, warp::Rejection> {
    db_mutex.lock().map_or_else(
        |err| {
            Err(warp::reject::custom(rejections::InternalServerError::from(
                err.to_string(),
            )))
        },
        |database| match auth::try_user_auth(
            &credentials.username,
            &credentials.password,
            &database,
        ) {
            Ok(session) => Ok(warp::reply::with_status(
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
            )),
            Err(err) => match err.downcast_ref::<Error>() {
                Some(err) => {
                    if matches!(err, Error::QueryReturnedNoRows) {
                        Err(warp::reject::custom(rejections::BadRequest))
                    } else {
                        error!("Error: {err:?}");
                        Err(warp::reject::custom(rejections::InternalServerError::from(
                            err.to_string(),
                        )))
                    }
                }
                None => Err(warp::reject::custom(rejections::Unauthorized)),
            },
        },
    )
}

#[allow(clippy::unused_async)]
pub async fn user_info(user: User) -> Result<impl warp::Reply, warp::Rejection> {
    Ok(warp::reply::json(&user))
}

#[allow(clippy::unused_async)]
pub async fn handle_rejection(
    err: warp::Rejection,
) -> Result<impl warp::Reply, std::convert::Infallible> {
    let code;
    let message;

    #[derive(Serialize)]
    struct ErrorMessage {
        code: u16,
        message: String,
    }

    if err.find::<rejections::NotFound>().is_some() {
        code = StatusCode::NOT_FOUND;
        message = "not found";
    } else if let Some(error) = err.find::<rejections::InternalServerError>() {
        error!("{}", error.error);
        code = StatusCode::INTERNAL_SERVER_ERROR;
        message = "internal server error";
    } else if err.find::<rejections::InvalidBearerToken>().is_some() {
        code = StatusCode::UNAUTHORIZED;
        message = "invalid brearer token";
    } else if err.find::<rejections::Unauthorized>().is_some() {
        code = StatusCode::UNAUTHORIZED;
        message = "unauthorized";
    } else if err.find::<rejections::BadRequest>().is_some() {
        code = StatusCode::BAD_REQUEST;
        message = "bad request";
    } else if err.find::<rejections::NotImplemented>().is_some() {
        code = StatusCode::NOT_IMPLEMENTED;
        message = "not implemented";
    } else if err.find::<warp::reject::InvalidQuery>().is_some() {
        code = StatusCode::BAD_REQUEST;
        message = "invalid query";
    } else if err.find::<warp::reject::InvalidHeader>().is_some() {
        code = StatusCode::BAD_REQUEST;
        message = "invalid header";
    } else if err.find::<warp::reject::LengthRequired>().is_some() {
        code = StatusCode::LENGTH_REQUIRED;
        message = "length required";
    } else if err.find::<warp::reject::MethodNotAllowed>().is_some() {
        code = StatusCode::METHOD_NOT_ALLOWED;
        message = "method not allowed";
    } else if err.find::<warp::reject::PayloadTooLarge>().is_some() {
        code = StatusCode::PAYLOAD_TOO_LARGE;
        message = "payload too large";
    } else if err.find::<warp::reject::UnsupportedMediaType>().is_some() {
        code = StatusCode::UNSUPPORTED_MEDIA_TYPE;
        message = "unsupported media type";
    } else {
        error!("unhandled rejection: {err:?}");
        code = StatusCode::IM_A_TEAPOT;
        message = "unhandled rejection";
    }

    let json = warp::reply::json(&ErrorMessage {
        code: code.as_u16(),
        message: message.into(),
    });

    Ok(warp::reply::with_status(json, code))
}
