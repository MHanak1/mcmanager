use crate::api::util;
use crate::api::util::rejections;
use crate::api::{auth, filters};
use crate::database::Database;
use crate::database::objects::{
    DbObject, InviteLink, Mod, ModLoader, Session, User, Version, World,
};
use crate::database::types::Id;
use rusqlite::Error;
use serde::de::DeserializeOwned;
use std::sync::{Arc, Mutex};
use warp::http::StatusCode;
use warp::{Filter};

pub trait ApiList: DbObject
where
    Self: std::marker::Sized,
    Self: serde::Serialize,
{
    //in theory the user filter should be done within the sql query, but for the sake of simplicity we do that when collecting the results
    fn api_list(
        db_mutex: Arc<Mutex<Database>>,
        user: User,
    ) -> Result<impl warp::Reply, warp::Rejection> {
        db_mutex.lock().map_or_else(
            |_| Err(warp::reject::custom(rejections::InternalServerError)),
            |database| {
                match database.get_all::<Self>(user) {
                    Ok(objects) => Ok(warp::reply::with_status(
                        warp::reply::json(&objects),
                        StatusCode::OK,
                    )),
                    Err(err) => {
                        match err {
                            //if the user does not have access to anything, instead of erroring out return an empty array
                            Error::QueryReturnedNoRows => Ok(warp::reply::with_status(
                                warp::reply::json::<std::vec::Vec<&str>>(&vec![]),
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
            .and_then(|db_mutex, user| async move { Self::api_list(db_mutex, user) })
    }
}
pub trait ApiGet: DbObject
where
    Self: std::marker::Sized,
    Self: serde::Serialize,
{
    fn api_get(
        id: String,
        db_mutex: Arc<Mutex<Database>>,
        user: User,
    ) -> Result<impl warp::Reply, warp::Rejection> {
        let id = Id::from_string(&id);
        if id.is_err() {
            return Err(warp::reject::custom(util::rejections::NotFound));
        }

        let id = id.unwrap();

        db_mutex.lock().map_or_else(
            |_| Err(warp::reject::not_found()),
            |database| match Self::get_from_db(&database.conn, id) {
                Ok(object) => {
                    if object.can_access(&user) {
                        Ok(warp::reply::with_status(
                            warp::reply::json(&object),
                            StatusCode::OK,
                        ))
                    } else {
                        // act as if the object doesn't exist
                        Err(warp::reject::custom(util::rejections::NotFound))
                    }
                }
                Err(err) => match err {
                    Error::QueryReturnedNoRows => {
                        Err(warp::reject::custom(util::rejections::NotFound))
                    }
                    _ => Err(warp::reject::custom(rejections::InternalServerError)),
                },
            },
        )
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

pub trait ApiCreate: DbObject
where
    Self: std::marker::Sized,
    Self: serde::Serialize,
{
    type JsonFrom: Clone + DeserializeOwned + Send;

    fn from_json(data: Self::JsonFrom, user: User) -> Self;
    //async fn api_create(db_mutex: Arc<Mutex<Database>>, user: User, data: Self::JsonFrom) -> Result<impl warp::Reply, warp::Rejection>;
    fn api_create(
        db_mutex: Arc<Mutex<Database>>,
        user: User,
        data: Self::JsonFrom,
    ) -> Result<impl warp::Reply, warp::Rejection> {
        if Self::can_create(&user) {
            db_mutex.lock().map_or_else(
                |_| Err(warp::reject::custom(rejections::InternalServerError)),
                |database| {
                    let new = Self::from_json(data, user);

                    match database.insert(&new) {
                        Ok(_) => Ok(warp::reply::with_status(
                            warp::reply::json(&new),
                            warp::http::StatusCode::CREATED,
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

/*
pub trait ApiUpdate : DbObject where Self: std::marker::Sized, Self: serde::Serialize {
    type JsonFrom;

    async fn api_create(db_mutex: Arc<Mutex<Database>>, user: User, data: Self::JsonFrom) -> Result<impl warp::Reply, warp::Rejection> {

    }
}
 */

mod json_fields {
    use crate::database::types::{Id};
    use serde::{Deserialize};

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
        pub username: String,
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
impl ApiList for Version {}
impl ApiList for ModLoader {}
impl ApiList for World {}
impl ApiList for User {}
impl ApiList for Session {}
impl ApiList for InviteLink {}

impl ApiGet for Mod {}
impl ApiGet for Version {}
impl ApiGet for ModLoader {}
impl ApiGet for World {}
impl ApiGet for User {}
impl ApiGet for Session {}
impl ApiGet for InviteLink {}

impl ApiCreate for Mod {
    type JsonFrom = json_fields::Mod;

    fn from_json(data: Self::JsonFrom, user: User) -> Self {
        Self {
            id: Default::default(),
            version_id: data.version_id,
            name: data.name,
            description: data.description.unwrap_or_default(),
            icon_id: data.icon_id,
            owner_id: user.id,
        }
    }
}

impl ApiCreate for Version {
    type JsonFrom = json_fields::Version;

    fn from_json(data: Self::JsonFrom, _user: User) -> Self {
        Self {
            id: Default::default(),
            minecraft_version: data.minecraft_version,
            mod_loader_id: data.mod_loader_id,
        }
    }
}

impl ApiCreate for ModLoader {
    type JsonFrom = json_fields::ModLoader;

    fn from_json(data: Self::JsonFrom, _user: User) -> Self {
        Self {
            id: Default::default(),
            name: data.name,
            can_load_mods: data.can_load_mods,
        }
    }
}

impl ApiCreate for World {
    type JsonFrom = json_fields::World;
    fn from_json(data: Self::JsonFrom, user: User) -> Self {
        Self {
            id: Default::default(),
            owner_id: user.id,
            name: data.name,
            icon_id: None,
            allocated_memory: data.allocated_memory.unwrap_or(1024),
            version_id: data.version_id,
            enabled: false,
        }
    }
}

impl ApiCreate for User {
    type JsonFrom = json_fields::User;

    fn from_json(data: Self::JsonFrom, _user: User) -> Self {
        Self {
            id: Default::default(),
            name: data.username,
            avatar_id: data.avatar_id,
            memory_limit: data.memory_limit,
            player_limit: data.player_limit,
            world_limit: data.world_limit,
            active_world_limit: data.active_world_limit,
            storage_limit: data.storage_limit,
            is_privileged: data.is_privileged.unwrap_or(false),
            enabled: data.enabled.unwrap_or(true),
        }
    }

    fn api_create(
        db_mutex: Arc<Mutex<Database>>,
        user: User,
        data: Self::JsonFrom,
    ) -> Result<impl warp::Reply, warp::Rejection> {
        if Self::can_create(&user) {
            db_mutex.lock().map_or_else(
                |_| Err(warp::reject::custom(rejections::InternalServerError)),
                |database| match database
                    .create_user_from(Self::from_json(data.clone(), user), data.password)
                {
                    Ok(new) => Ok(warp::reply::with_status(
                        warp::reply::json(&new),
                        warp::http::StatusCode::CREATED,
                    )),
                    Err(err) => match err.downcast_ref::<rusqlite::Error>() {
                        Some(err) => if let Error::SqliteFailure(err, ..) = err { if let rusqlite::ffi::Error {
                                code: rusqlite::ErrorCode::ConstraintViolation,
                                ..
                            } = *err { Ok(warp::reply::with_status(
                            warp::reply::json(&"username already taken"),
                            StatusCode::CONFLICT,
                        )) } else {
                            eprintln!("{err:?}");
                            Err(warp::reject::custom(rejections::InternalServerError))
                        } } else {
                            eprintln!("{err:?}");
                            Err(warp::reject::custom(rejections::InternalServerError))
                        },
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

impl ApiCreate for Session {
    type JsonFrom = json_fields::Session;
    fn from_json(data: Self::JsonFrom, user: User) -> Self {
        Self {
            user_id: user.id,
            token: Default::default(),
            created: chrono::offset::Utc::now(),
            expires: data.expires.unwrap_or(true),
        }
    }
}

impl ApiCreate for InviteLink {
    type JsonFrom = json_fields::InviteLink;

    fn from_json(_data: Self::JsonFrom, user: User) -> Self {
        Self {
            id: Default::default(),
            invite_token: Default::default(),
            creator_id: user.id,
            created: chrono::offset::Utc::now(),
        }
    }
}

//this in theory could be transformed into ApiCreate implementation, but it would require a fair amount of changes, and for now it's not causing any problems
pub async fn user_auth(
    db_mutex: Arc<Mutex<Database>>,
    credentials: json_fields::Login,
) -> Result<impl warp::Reply, warp::Rejection> {
    db_mutex.lock().map_or_else(
        |_| Err(warp::reject::custom(util::rejections::InternalServerError)),
        |database| match auth::try_user_auth(credentials.username, credentials.password, &database)
        {
            Ok(session) => Ok(warp::reply::with_status(
                warp::reply::with_header(
                    warp::reply::json(&session.token),
                    "set-cookie",
                    format!("auth={}; Path=/; HttpOnly; Max-Age=1209600", session.token),
                ),
                StatusCode::CREATED,
            )),
            Err(err) => match err.downcast_ref::<rusqlite::Error>() {
                Some(err) => {
                    if matches!(err, rusqlite::Error::QueryReturnedNoRows) {
                        Err(warp::reject::custom(util::rejections::BadRequest))
                    } else {
                        eprintln!("Error: {err:?}");
                        Err(warp::reject::custom(util::rejections::InternalServerError))
                    }
                }
                None => Err(warp::reject::custom(util::rejections::Unauthorized)),
            },
        },
    )
}

pub async fn user_info(user: User) -> Result<impl warp::Reply, warp::Rejection> {
    Ok(warp::reply::json(&user))
}
