use crate::api::util::rejections;
use crate::api::{auth, filters};
use crate::database::Database;
use crate::database::objects::{
    DbObject, FromJson, InviteLink, Mod, ModLoader, Session, UpdateJson, User, Version, World,
};
use crate::database::types::Id;
use rusqlite::Error;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use warp::Filter;
use warp::http::StatusCode;

pub trait ApiList: DbObject
where
    Self: Sized,
    Self: serde::Serialize,
{
    //in theory the user filter should be done within the sql query, but for the sake of simplicity we do that when collecting the results
    fn api_list(
        db_mutex: Arc<Mutex<Database>>,
        user: User,
        filters: HashMap<String, String>,
    ) -> Result<impl warp::Reply, warp::Rejection> {
        db_mutex.lock().map_or_else(
            |_| Err(warp::reject::custom(rejections::InternalServerError)),
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
                                eprintln!("{err:?}");
                                Err(warp::reject::custom(rejections::InternalServerError))
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
    Self: serde::Serialize,
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
                            eprintln!("{err:?}");
                            Err(warp::reject::custom(rejections::InternalServerError))
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
    Self: serde::Serialize,
{
    fn api_create(
        db_mutex: Arc<Mutex<Database>>,
        user: User,
        data: Self::JsonFrom,
    ) -> Result<impl warp::Reply, warp::Rejection> {
        if Self::can_create(&user) {
            db_mutex.lock().map_or_else(
                |_| Err(warp::reject::custom(rejections::InternalServerError)),
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
                            eprintln!("{err:?}");
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
            .and(warp::path("create"))
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
            |_| Err(warp::reject::custom(rejections::InternalServerError)),
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
                                    eprintln!("{err:?}");
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
            .and(warp::path("update"))
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
                                eprintln!("{err:?}");
                                Err(warp::reject::custom(rejections::InternalServerError))
                            }
                        },
                    },
                    Err(err) => match err {
                        Error::QueryReturnedNoRows => {
                            Err(warp::reject::custom(rejections::NotFound))
                        }
                        _ => {
                            eprintln!("{err:?}");
                            Err(warp::reject::custom(rejections::InternalServerError))
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
        warp::get()
            .and(warp::path("api"))
            .and(warp::path(Self::table_name()))
            .and(warp::path::param::<String>())
            .and(warp::path("remove"))
            .and(warp::path::end())
            .and(filters::with_db(db_mutex.clone()))
            .and(filters::with_auth(db_mutex))
            .and_then(|id, db_mutex, user| async move { Self::api_remove(id, db_mutex, user) })
    }
}

pub(crate) mod json_fields {
    use crate::database::types::Id;
    use serde::Deserialize;

    #[derive(Debug, Clone, Deserialize)]
    pub struct Login {
        pub username: String,
        pub password: String,
    }

    #[derive(Debug, Clone, Deserialize)]
    pub struct Mod {
        pub version_id: Id,
        pub name: String,
        pub description: Option<String>,
        pub icon_id: Option<Id>,
    }

    #[allow(clippy::struct_field_names)]
    #[derive(Debug, Clone, Deserialize)]
    pub struct Version {
        pub minecraft_version: String,
        pub mod_loader_id: Id,
    }

    #[derive(Debug, Clone, Deserialize)]
    pub struct ModLoader {
        pub name: String,
        pub can_load_mods: bool,
    }

    #[derive(Debug, Clone, Deserialize)]
    pub struct World {
        pub name: String,
        pub icon_id: Option<Id>,
        pub allocated_memory: Option<u32>,
        pub version_id: Id,
        pub enabled: Option<bool>,
    }

    #[derive(Debug, Clone, Deserialize)]
    pub struct User {
        pub name: String,
        pub password: String,
        pub avatar_id: Option<Id>,
        pub memory_limit: Option<u32>,
        pub player_limit: Option<u32>,
        pub world_limit: Option<u32>,
        pub active_world_limit: Option<u32>,
        pub storage_limit: Option<u32>,
        pub is_privileged: Option<bool>,
        pub enabled: Option<bool>,
    }

    #[derive(Debug, Clone, Deserialize)]
    pub struct Session {
        pub expires: Option<bool>,
    }

    #[derive(Debug, Clone, Deserialize)]
    pub struct InviteLink {}
}

impl ApiList for Mod {}
impl ApiGet for Mod {}
impl ApiCreate for Mod {}
impl ApiUpdate for Mod {}
impl ApiRemove for Mod {}

impl ApiList for Version {}
impl ApiGet for Version {}
impl ApiCreate for Version {}
impl ApiUpdate for Version {}
impl ApiRemove for Version {}

impl ApiList for ModLoader {}
impl ApiGet for ModLoader {}
impl ApiCreate for ModLoader {}
impl ApiUpdate for ModLoader {}
impl ApiRemove for ModLoader {}

impl ApiList for World {}
impl ApiGet for World {}
impl ApiCreate for World {}
impl ApiUpdate for World {}
impl ApiRemove for World {}

impl ApiList for User {}
impl ApiGet for User {}
impl ApiCreate for User {
    fn api_create(
        db_mutex: Arc<Mutex<Database>>,
        user: User,
        data: Self::JsonFrom,
    ) -> Result<impl warp::Reply, warp::Rejection> {
        if Self::can_create(&user) {
            db_mutex.lock().map_or_else(
                |_| Err(warp::reject::custom(rejections::InternalServerError)),
                |database| match database
                    .create_user_from(Self::from_json(data.clone(), user), &data.password)
                {
                    Ok(new) => Ok(warp::reply::with_status(
                        warp::reply::json(&new),
                        StatusCode::CREATED,
                    )),
                    Err(err) => match err.downcast_ref::<Error>() {
                        Some(err) => {
                            if let Error::SqliteFailure(err, ..) = err {
                                if let rusqlite::ffi::Error {
                                    code: rusqlite::ErrorCode::ConstraintViolation,
                                    ..
                                } = *err
                                {
                                    Ok(warp::reply::with_status(
                                        warp::reply::json(&"username already taken"),
                                        StatusCode::CONFLICT,
                                    ))
                                } else {
                                    eprintln!("{err:?}");
                                    Err(warp::reject::custom(rejections::InternalServerError))
                                }
                            } else {
                                eprintln!("{err:?}");
                                Err(warp::reject::custom(rejections::InternalServerError))
                            }
                        }
                        _ => Err(warp::reject::custom(rejections::InternalServerError)),
                    },
                },
            )
        } else {
            Ok(warp::reply::with_status(
                warp::reply::json(&"Unauthorized"),
                StatusCode::UNAUTHORIZED,
            ))
        }
    }
}
impl ApiUpdate for User {}
impl ApiRemove for User {}

impl ApiList for Session {}
impl ApiGet for Session {}
impl ApiCreate for Session {}
impl ApiRemove for Session {}

impl ApiList for InviteLink {}
impl ApiGet for InviteLink {}
impl ApiCreate for InviteLink {}
impl ApiRemove for InviteLink {}

//this in theory could be transformed into ApiCreate implementation, but it would require a fair amount of changes, and for now it's not causing any problems
#[allow(clippy::unused_async)]
pub async fn user_auth(
    db_mutex: Arc<Mutex<Database>>,
    credentials: json_fields::Login,
) -> Result<impl warp::Reply, warp::Rejection> {
    db_mutex.lock().map_or_else(
        |_| Err(warp::reject::custom(rejections::InternalServerError)),
        |database| match auth::try_user_auth(
            &credentials.username,
            &credentials.password,
            &database,
        ) {
            Ok(session) => Ok(warp::reply::with_status(
                warp::reply::with_header(
                    warp::reply::json(&session.token),
                    "set-cookie",
                    format!("auth={}; Path=/; HttpOnly; Max-Age=1209600", session.token),
                ),
                StatusCode::CREATED,
            )),
            Err(err) => match err.downcast_ref::<Error>() {
                Some(err) => {
                    if matches!(err, Error::QueryReturnedNoRows) {
                        Err(warp::reject::custom(rejections::BadRequest))
                    } else {
                        eprintln!("Error: {err:?}");
                        Err(warp::reject::custom(rejections::InternalServerError))
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
