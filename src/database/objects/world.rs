use crate::api::filters;
use crate::api::handlers::{ApiCreate, ApiGet, ApiList, ApiObject, ApiRemove, ApiUpdate};
use crate::api::util::rejections;
use crate::database::objects::{DbObject, FromJson, UpdateJson, User};
use crate::database::types::{Access, Column, Id, Type};
use crate::database::{Database, DatabaseError};
use crate::minecraft;
use crate::minecraft::server;
use crate::minecraft::server::MinecraftServerStatus;
use crate::minecraft::server::internal::InternalServer;
use rusqlite::types::ToSqlOutput;
use rusqlite::{Row, ToSql};
use serde::{Deserialize, Deserializer, Serialize};
use std::sync::{Arc, Mutex};
use warp::http::StatusCode;
use warp::{Filter, Rejection, Reply, reject};
use warp_rate_limit::{RateLimitConfig, RateLimitInfo};

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct World {
    /// world's unique [`Id`]
    pub id: Id,
    /// references [`User`]
    pub owner_id: Id,
    /// world's name
    pub name: String,
    /// the subdomain the the server will be under
    pub hostname: String,
    /// id of the icon stored in the filesystem (data/icons)
    pub icon_id: Option<Id>,
    /// amount of memory allocated to the server in MiB
    pub allocated_memory: u32,
    /// references [`Version`]
    pub version_id: Id,
    /// whether a server hosting this world should be running or not
    pub enabled: bool,
}

impl DbObject for World {
    fn view_access() -> Access {
        Access::Owner("owner_id").or(Access::PrivilegedUser)
    }

    fn update_access() -> Access {
        Access::Owner("owner_id").or(Access::PrivilegedUser)
    }

    fn create_access() -> Access {
        Access::User
    }

    fn table_name() -> &'static str {
        "worlds"
    }

    fn columns() -> Vec<Column> {
        vec![
            Column::new("id", Type::Id).primary_key(),
            Column::new("owner_id", Type::Id)
                .not_null()
                .references("users(id)"),
            Column::new("name", Type::Text).not_null(),
            Column::new("hostname", Type::Text).not_null().unique(),
            Column::new("icon_id", Type::Id),
            Column::new("allocated_memory", Type::Integer(false)).not_null(),
            Column::new("version_id", Type::Id)
                .not_null()
                .references("versions(id)"),
            Column::new("enabled", Type::Boolean)
                .not_null()
                .default("false"),
        ]
    }

    fn from_row(row: &Row) -> rusqlite::Result<Self>
    where
        Self: Sized,
    {
        Ok(Self {
            id: row.get(0)?,
            owner_id: row.get(1)?,
            name: row.get(2)?,
            hostname: row.get(3)?,
            icon_id: row.get(4)?,
            allocated_memory: row.get(5)?,
            version_id: row.get(6)?,
            enabled: row.get(7)?,
        })
    }
    fn get_id(&self) -> Id {
        self.id
    }
    fn params(&self) -> Vec<ToSqlOutput> {
        vec![
            self.id
                .to_sql()
                .expect("failed to convert the value to sql"),
            self.owner_id
                .to_sql()
                .expect("failed to convert the value to sql"),
            self.name
                .to_sql()
                .expect("failed to convert the value to sql"),
            self.hostname
                .to_sql()
                .expect("failed to convert the value to sql"),
            self.icon_id
                .to_sql()
                .expect("failed to convert the value to sql"),
            self.allocated_memory
                .to_sql()
                .expect("failed to convert the value to sql"),
            self.version_id
                .to_sql()
                .expect("failed to convert the value to sql"),
            self.enabled
                .to_sql()
                .expect("failed to convert the value to sql"),
        ]
    }
}
/*
impl World {
    pub fn status(&self) -> MinecraftServerStatus {
        match minecraft::server::SERVERS
            .lock()
            .expect("couldn't get servers")
            .iter()
            .find_map(|server| {
                if server.id() == self.id {
                    Some(server)
                } else {
                    None
                }
            }) {
            Some(server) => *server.status(),
            None => MinecraftServerStatus::Exited(0),
        }
    }
}
*/

#[allow(unused)]
fn is_valid_hostname(hostname: &str) -> bool {
    //in theory this could be done through regex, but this is simpler and i don't want to add a new dependency just for this
    const ALLOWED_CHARS: &str = "abcdefghijklmnopqrstuvwzyz01234567890";

    for char in hostname.chars() {
        if !ALLOWED_CHARS.contains(char) {
            return false;
        }
    }
    true
}

fn into_valid_hostname(hostname: &str) -> String {
    const ALLOWED_CHARS: &str = "abcdefghijklmnopqrstuvwzyz01234567890-";

    let hostname = hostname.to_ascii_lowercase();
    let mut new_hostname = String::with_capacity(hostname.len());

    for char in hostname.chars() {
        if ALLOWED_CHARS.contains(char) {
            new_hostname.push(char);
        } else if char.is_whitespace() {
            new_hostname.push('-');
        }
    }
    new_hostname
}

// Any value that is present is considered Some value, including null.
fn deserialize_some<'de, T, D>(deserializer: D) -> Result<Option<T>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    Deserialize::deserialize(deserializer).map(Some)
}

#[derive(Debug, Clone, Deserialize)]
pub struct JsonFrom {
    pub name: String,
    pub hostname: String,
    pub icon_id: Option<Id>,
    pub allocated_memory: Option<u32>,
    pub version_id: Id,
}

impl FromJson for World {
    type JsonFrom = JsonFrom;
    fn from_json(data: &Self::JsonFrom, user: &User) -> Self {
        Self {
            id: Id::default(),
            owner_id: user.id,
            name: data.name.clone(),
            hostname: into_valid_hostname(&data.hostname),
            icon_id: data.icon_id,
            allocated_memory: data
                .allocated_memory
                .unwrap_or(crate::config::CONFIG.world_defaults.allocated_memory),
            version_id: data.version_id,
            enabled: false,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct JsonUpdate {
    pub name: Option<String>,
    pub hostname: Option<String>,
    #[serde(default, deserialize_with = "deserialize_some")]
    pub icon_id: Option<Option<Id>>,
    pub allocated_memory: Option<u32>,
    pub version_id: Option<Id>,
    pub enabled: Option<bool>,
}
impl UpdateJson for World {
    type JsonUpdate = JsonUpdate;
    fn update_with_json(&self, data: &Self::JsonUpdate) -> Self {
        let mut new = self.clone();
        new.name = data.name.clone().unwrap_or(new.name);
        new.hostname = data.hostname.clone().unwrap_or(new.hostname);
        new.icon_id = data.icon_id.unwrap_or(new.icon_id);
        new.allocated_memory = data.allocated_memory.unwrap_or(new.allocated_memory);
        new.version_id = data.version_id.unwrap_or(new.version_id);
        new.enabled = data.enabled.unwrap_or(new.enabled);
        new
    }
}

impl ApiObject for World {
    fn filters(
        db_mutex: Arc<Mutex<Database>>,
        rate_limit_config: RateLimitConfig,
    ) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone {
        Self::list_filter(db_mutex.clone(), rate_limit_config.clone())
            .or(Self::status_filter(
                db_mutex.clone(),
                rate_limit_config.clone(),
            ))
            .or(Self::config_filter(
                db_mutex.clone(),
                rate_limit_config.clone(),
            ))
            .or(Self::get_filter(
                db_mutex.clone(),
                rate_limit_config.clone(),
            ))
            .or(Self::create_filter(
                db_mutex.clone(),
                rate_limit_config.clone(),
            ))
            .or(Self::update_filter(
                db_mutex.clone(),
                rate_limit_config.clone(),
            ))
            .or(Self::remove_filter(
                db_mutex.clone(),
                rate_limit_config.clone(),
            ))
    }
}

impl ApiList for World {}
impl ApiGet for World {}
impl ApiCreate for World {
    fn before_api_create(
        database: &Database,
        json: &mut Self::JsonFrom,
    ) -> Result<(), DatabaseError> {
        json.hostname = into_valid_hostname(&json.hostname);
        if !database
            .list_filtered::<World>(
                vec![(String::from("hostname"), json.hostname.clone())],
                None,
            )?
            .is_empty()
        {
            json.hostname += &rand::random_range(0..100000).to_string();
        }
        Ok(())
    }
    fn after_api_create(
        &self,
        _database: &Database,
        _json: &mut Self::JsonFrom,
    ) -> Result<(), DatabaseError> {
        minecraft::server::internal::InternalServer::new(self)
            .map_err(|err| DatabaseError::InternalServerError(err.to_string()))?;
        Ok(())
    }
}
impl ApiUpdate for World {
    fn before_api_update(
        &self,
        database: &Database,
        json: &mut Self::JsonUpdate,
    ) -> Result<(), DatabaseError> {
        if let Some(hostname) = &json.hostname {
            json.hostname = Some(into_valid_hostname(hostname))
        }
        if let Some(hostname) = &json.hostname {
            if database
                .list_filtered::<World>(
                    vec![
                        (String::from("hostname"), hostname.clone()),
                        (String::from("id"), format!("!{}", self.id.as_i64())),
                    ],
                    None,
                )?
                .is_empty()
            {
                json.hostname = Some(hostname.clone());
            } else {
                json.hostname = Some(hostname.clone() + &rand::random_range(0..100000).to_string());
            }
        }
        Ok(())
    }
    fn after_api_update(
        &self,
        _database: &Database,
        _json: &mut Self::JsonUpdate,
    ) -> Result<(), DatabaseError> {
        let mut server = server::get_server(self.id);
        if server.is_none() {
            server::add_server(Box::new(
                InternalServer::new(self)
                    .map_err(|err| DatabaseError::InternalServerError(err.to_string()))?,
            ))
            .map_err(|err| DatabaseError::InternalServerError(err.to_string()))?;
            server = server::get_server(self.id);
            assert!(server.is_some());
        }
        let server = server.unwrap();
        let mut server = server.lock().expect("failed to lock server");

        server
            .update_world(self.clone())
            .map_err(|err| DatabaseError::InternalServerError(err.to_string()))
        /*
        if self.enabled {
            server
                .start()
                .map_err(|err| DatabaseError::InternalServerError(err.to_string()))?;
            Ok(())
        } else {
            server
                .stop()
                .map_err(|err| DatabaseError::InternalServerError(err.to_string()))?;
            Ok(())
        }
         */
    }
}
impl ApiRemove for World {}

impl World {
    #[allow(clippy::needless_pass_by_value)]
    fn world_get_status(
        _rate_limit_info: RateLimitInfo,
        id: String,
        db_mutex: Arc<Mutex<Database>>,
        user: User,
    ) -> Result<impl warp::Reply, warp::Rejection> {
        let id = Id::from_string(&id).map_err(|_| reject::custom(rejections::NotFound))?;
        let database = db_mutex.lock().map_err(|err| {
            reject::custom(rejections::InternalServerError::from(err.to_string()))
        })?;

        database
            .get_one::<Self>(id, Some(&user))
            .map_err(crate::api::handlers::handle_database_error)?;

        let server = server::get_server(id);
        let status = match server {
            Some(server) => server.lock().expect("failed to get server").status(),
            None => Ok(MinecraftServerStatus::Exited(0)),
        }
        .map_err(|err| warp::reject::custom(rejections::InternalServerError::from(err)))?;

        #[derive(Serialize)]
        struct Status {
            status: String,
            code: u32,
        }

        let status = match status {
            MinecraftServerStatus::Running => Status {
                status: "running".to_string(),
                code: 0,
            },
            MinecraftServerStatus::Exited(code) => Status {
                status: "exited".to_string(),
                code,
            },
        };

        Ok(warp::reply::with_status(
            warp::reply::json(&status),
            StatusCode::OK,
        ))
    }

    pub fn status_filter(
        db_mutex: Arc<Mutex<Database>>,
        rate_limit_config: RateLimitConfig,
    ) -> impl Filter<Extract = (impl warp::Reply,), Error = warp::Rejection> + Clone {
        warp::path("api")
            .and(warp_rate_limit::with_rate_limit(rate_limit_config))
            .and(warp::path(Self::table_name()))
            .and(warp::path::param::<String>())
            .and(warp::path("status"))
            .and(warp::path::end())
            .and(warp::get())
            .and(filters::with_db(db_mutex.clone()))
            .and(filters::with_auth(db_mutex))
            .and_then(|rate_limit_info, id, db_mutex, user| async move {
                Self::world_get_status(rate_limit_info, id, db_mutex, user)
            })
    }

    #[allow(clippy::needless_pass_by_value)]
    fn get_server_config(
        _rate_limit_info: RateLimitInfo,
        id: String,
        _db_mutex: Arc<Mutex<Database>>,
        _user: User,
    ) -> Result<impl warp::Reply, warp::Rejection> {
        let id = Id::from_string(&id).map_err(|_| reject::custom(rejections::NotFound))?;
        //TODO: check if the player can actually access this
        /*
        let database = db_mutex.lock().map_err(|err| {
            reject::custom(rejections::InternalServerError::from(err.to_string()))
        })?;
        let world = database
            .get_one::<Self>(id, Some(&user))
            .map_err(crate::api::handlers::handle_database_error)?;

         */

        let server = match server::get_server(id) {
            Some(server) => server,
            //TODO: if the server is not running, then it should be created, and then its config read.
            None => return Err(warp::reject::custom(rejections::NotFound)),
        };

        let server = server
            .lock()
            .map_err(|err| warp::reject::custom(rejections::InternalServerError::from(err)))?;

        let config = server
            .config()
            .map_err(|err| warp::reject::custom(rejections::InternalServerError::from(err)))?;

        Ok(warp::reply::with_status(
            warp::reply::json(&config),
            StatusCode::OK,
        ))
    }

    pub fn config_filter(
        db_mutex: Arc<Mutex<Database>>,
        rate_limit_config: RateLimitConfig,
    ) -> impl Filter<Extract = (impl warp::Reply,), Error = warp::Rejection> + Clone {
        warp::path("api")
            .and(warp_rate_limit::with_rate_limit(rate_limit_config))
            .and(warp::path(Self::table_name()))
            .and(warp::path::param::<String>())
            .and(warp::path("config"))
            .and(warp::path::end())
            .and(warp::get())
            .and(filters::with_db(db_mutex.clone()))
            .and(filters::with_auth(db_mutex))
            .and_then(|rate_limit_info, id, db_mutex, user| async move {
                Self::get_server_config(rate_limit_info, id, db_mutex, user)
            })
    }
}
