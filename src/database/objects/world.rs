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
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::sync::{Arc, Mutex};
use warp::http::StatusCode;
use warp::{Filter, Rejection, Reply, reject};
use warp_rate_limit::{RateLimitConfig, RateLimitInfo};

/// `id`: world's unique [`Id`]
///
/// `owner_id`: references [`User`]
///
/// `name`: world's name
///
/// `icon_id`: id of the icon stored in the filesystem (data/icons)
///
/// `allocated_memory`: amount of memory allocated to the server in MiB
///
/// `version_id`: references [`Version`]
///
/// `enabled`: whether a server hosting this world should be running or not
#[derive(Debug, PartialEq, Eq, Clone, Deserialize)]
pub struct World {
    pub id: Id,
    pub owner_id: Id,
    pub name: String,
    pub icon_id: Option<Id>,
    pub allocated_memory: u32,
    pub version_id: Id,
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
            Column::new("icon_id", Type::Id),
            Column::new("allocated_memory", Type::Integer(false)),
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
            icon_id: row.get(3)?,
            allocated_memory: row.get(4)?,
            version_id: row.get(5)?,
            enabled: row.get(6)?,
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
            icon_id: data.icon_id,
            allocated_memory: data
                .allocated_memory
                .unwrap_or(crate::config::CONFIG.world_defaults.allocated_memory),
            version_id: data.version_id,
            enabled: false,
        }
    }
}

#[derive(Serialize)]
pub struct JsonTo {
    pub id: Id,
    pub owner_id: Id,
    pub name: String,
    pub icon_id: Option<Id>,
    pub allocated_memory: u32,
    pub version_id: Id,
    pub enabled: bool,
}

impl Serialize for World {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        JsonTo {
            id: self.id,
            owner_id: self.owner_id,
            name: self.name.clone(),
            icon_id: self.icon_id,
            allocated_memory: self.allocated_memory,
            version_id: self.version_id,
            enabled: self.enabled,
        }
        .serialize(serializer)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct JsonUpdate {
    pub name: Option<String>,
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
    fn after_api_create(
        &self,
        _database: &Database,
        _json: &Self::JsonFrom,
    ) -> Result<(), DatabaseError> {
        minecraft::server::internal::InternalServer::new(self)
            .map_err(|err| DatabaseError::InternalServerError(err.to_string()))?;
        Ok(())
    }
}
impl ApiUpdate for World {
    fn after_api_update(
        &self,
        _database: &Database,
        _json: &Self::JsonUpdate,
    ) -> Result<(), DatabaseError> {
        let mut server = server::get_server(self.id);
        if server.is_none() {
            server::add_server(Box::new(
                InternalServer::new(self)
                    .map_err(|err| DatabaseError::InternalServerError(err.to_string()))?,
            ))
            .map_err(|err| DatabaseError::InternalServerError(err.to_string()))?;
            server = server::get_server(self.id);
        }
        let server = server.expect("what the fuck");
        let mut server = server.lock().expect("failed to lock server");

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
            Some(server) => *server.lock().expect("failed to get server").status(),
            None => MinecraftServerStatus::Exited(0),
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
}
