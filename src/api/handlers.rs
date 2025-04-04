use crate::database::Database;
use crate::database::objects::{DbObject, Mod, User, World};
use crate::database::types::Id;
use log::error;
use std::sync::{Arc, Mutex};
use warp::http::StatusCode;
use crate::api::auth;
use crate::api::util;

//in theory the user filter should be done within the sql query, but for the sake of simplicity we do that when collecting the results
pub async fn list_mods(
    db_mutex: Arc<Mutex<Database>>,
    user: User,
) -> Result<impl warp::Reply, warp::Rejection> {
    db_mutex.lock().map_or_else(
        |_| {
            Ok(warp::reply::with_status(
                warp::reply::json(&"internal server error".to_string()),
                StatusCode::INTERNAL_SERVER_ERROR,
            ))
        },
        |database| {
            match database.get_all::<Mod>(user) {
                //Ok(mods) => Ok(warp::reply::json(&mods)),
                Ok(mods) => Ok(warp::reply::with_status(
                    warp::reply::json(&mods),
                    StatusCode::OK,
                )),
                Err(err) => {
                    error!("{:?}", err);
                    Ok(warp::reply::with_status(
                        warp::reply::json(&"internal server error".to_string()),
                        StatusCode::INTERNAL_SERVER_ERROR,
                    ))
                }
            }
        },
    )
}
pub async fn get_mod(
    id: String,
    db_mutex: Arc<Mutex<Database>>,
    user: User,
) -> Result<impl warp::Reply, warp::Rejection> {
    let id = Id::from_string(&id);
    if id.is_err() {
        return Ok(warp::reply::with_status(
            warp::reply::json(&"not found".to_string()),
            StatusCode::NOT_FOUND,
        ));
    }

    let id = id.unwrap();

    db_mutex.lock().map_or_else(
        |_| Err(warp::reject::not_found()),
        |database| match Mod::get_from_db(&database.conn, id) {
            Ok(mcmod) => {
                if mcmod.is_accessible(&user) {
                    Ok(warp::reply::with_status(
                        warp::reply::json(&mcmod),
                        StatusCode::NOT_FOUND,
                    ))
                } else {
                    // act as if the mod doesn't exist
                    Ok(warp::reply::with_status(
                        warp::reply::json(&"not found".to_string()),
                        StatusCode::NOT_FOUND,
                    ))
                }
            }
            Err(_) => Ok(warp::reply::with_status(
                warp::reply::json(&"not found".to_string()),
                StatusCode::NOT_FOUND,
            )),
        },
    )
}

pub async fn list_worlds(
    db_mutex: Arc<Mutex<Database>>,
    user: User,
) -> Result<impl warp::Reply, warp::Rejection> {
    db_mutex.lock().map_or_else(
        |_| {
            Ok(warp::reply::with_status(
                warp::reply::json(&"internal server error".to_string()),
                StatusCode::INTERNAL_SERVER_ERROR,
            ))
        },
        |database| {
            match database.get_all::<World>(user) {
                //Ok(mods) => Ok(warp::reply::json(&mods)),
                Ok(worlds) => Ok(warp::reply::with_status(
                    warp::reply::json(&worlds),
                    StatusCode::OK,
                )),
                Err(err) => {
                    error!("{:?}", err);
                    Ok(warp::reply::with_status(
                        warp::reply::json(&"internal server error".to_string()),
                        StatusCode::INTERNAL_SERVER_ERROR,
                    ))
                }
            }
        },
    )
}

pub async fn user_auth(
    db_mutex: Arc<Mutex<Database>>,
    credentials: util::data_types::LoginCredentials,
) -> Result<impl warp::Reply, warp::Rejection> {
    db_mutex.lock().map_or_else(
        |_| Err(warp::reject::custom(util::rejections::InternalServerError)),
        |database| {
            match auth::try_user_auth(credentials.username, credentials.password, &database) {
                Ok(session) => {
                    Ok(warp::reply::with_status(warp::reply::with_header(warp::reply::json(&session.token), "set-cookie", format!("auth={}; Path=/; HttpOnly; Max-Age=1209600", session.token)), StatusCode::CREATED))
                }
                Err(error) => {
                    match error.downcast_ref::<rusqlite::Error>() {
                        Some(error) => {
                            if let rusqlite::Error::QueryReturnedNoRows = error {
                                Err(warp::reject::custom(util::rejections::Unauthorized))
                            } else {
                                println!("Error: {error:?}");
                                Err(warp::reject::custom(util::rejections::InternalServerError))
                            }
                        }
                        None => {
                            Err(warp::reject::custom(util::rejections::Unauthorized))
                        }
                    }
                }
            }
        }

    )
}