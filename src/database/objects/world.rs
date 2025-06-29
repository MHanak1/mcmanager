use crate::api::filters;
use crate::api::handlers::{ApiCreate, ApiGet, ApiList, ApiObject, ApiRemove, ApiUpdate};
use crate::api::util::rejections;
use crate::database::objects::group::Group;
use crate::database::objects::{DbObject, FromJson, UpdateJson, User, Version};
use crate::database::types::{Access, Column, Id};
use crate::database::{Database, DatabaseError, ValueType};
use crate::minecraft::server;
use crate::minecraft::server::{MinecraftServerStatus, ServerConfigLimit};
use async_trait::async_trait;
use log::{debug, info};
use serde::{Deserialize, Deserializer, Serialize};
use sqlx::{Any, Arguments, Encode, FromRow, IntoArguments};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::task::id;
use warp::http::StatusCode;
use warp::{Filter, Rejection, Reply, reject};
use warp_rate_limit::{RateLimitConfig, RateLimitInfo};

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, FromRow)]
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
    pub allocated_memory: i32,
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
            Column::new("id", ValueType::Id).primary_key(),
            Column::new("owner_id", ValueType::Id)
                .not_null()
                .references("users(id)"),
            Column::new("name", ValueType::Text).not_null(),
            Column::new("hostname", ValueType::Text).not_null().unique(),
            Column::new("icon_id", ValueType::Id),
            Column::new("allocated_memory", ValueType::Integer).not_null(),
            Column::new("version_id", ValueType::Id)
                .not_null()
                .references("versions(id)"),
            Column::new("enabled", ValueType::Boolean)
                .not_null()
                .default("false"),
        ]
    }

    fn id(&self) -> Id {
        self.id
    }
}

impl<'a> IntoArguments<'a, sqlx::Sqlite> for World {
    fn into_arguments(self) -> sqlx::sqlite::SqliteArguments<'a> {
        let mut arguments = sqlx::sqlite::SqliteArguments::default();
        arguments.add(self.id).expect("Failed to add argument");
        arguments
            .add(self.owner_id)
            .expect("Failed to add argument");
        arguments.add(self.name).expect("Failed to add argument");
        arguments
            .add(self.hostname)
            .expect("Failed to add argument");
        arguments.add(self.icon_id).expect("Failed to add argument");
        arguments
            .add(self.allocated_memory)
            .expect("Failed to add argument");
        arguments
            .add(self.version_id)
            .expect("Failed to add argument");
        arguments.add(self.enabled).expect("Failed to add argument");
        arguments
    }
}

impl<'a> IntoArguments<'a, sqlx::Postgres> for World {
    fn into_arguments(self) -> sqlx::postgres::PgArguments {
        let mut arguments = sqlx::postgres::PgArguments::default();
        arguments.add(self.id).expect("Failed to add argument");
        arguments
            .add(self.owner_id)
            .expect("Failed to add argument");
        arguments.add(self.name).expect("Failed to add argument");
        arguments
            .add(self.hostname)
            .expect("Failed to add argument");
        arguments.add(self.icon_id).expect("Failed to add argument");
        arguments
            .add(self.allocated_memory)
            .expect("Failed to add argument");
        arguments
            .add(self.version_id)
            .expect("Failed to add argument");
        arguments.add(self.enabled).expect("Failed to add argument");
        arguments
    }
}

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
                .unwrap_or(crate::config::CONFIG.world_defaults.allocated_memory)
                .try_into()
                .unwrap_or(i32::MAX),
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
        new.allocated_memory = data
            .allocated_memory
            .map(|v| v.try_into().unwrap_or(i32::MAX))
            .unwrap_or(new.allocated_memory);
        new.version_id = data.version_id.unwrap_or(new.version_id);
        new.enabled = data.enabled.unwrap_or(new.enabled);
        new
    }
}

impl ApiObject for World {
    fn filters(
        db_mutex: Arc<Database>,
        rate_limit_config: RateLimitConfig,
    ) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone {
        Self::list_filter(db_mutex.clone(), rate_limit_config.clone())
            .or(Self::status_filter(
                db_mutex.clone(),
                rate_limit_config.clone(),
            ))
            .or(Self::get_config_filter(
                db_mutex.clone(),
                rate_limit_config.clone(),
            ))
            .or(Self::set_config_filter(
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
#[async_trait]
impl ApiCreate for World {
    //TODO: this needs a rewrite, too much repeated code
    async fn before_api_create(
        database: Arc<Database>,
        json: &mut Self::JsonFrom,
        user: &User,
    ) -> Result<(), DatabaseError> {
        let group = user.group(database.clone(), None).await;
        let user_worlds: Vec<World> = database
            .get_all_where("owner_id", user.id, Some((&user, &group)))
            .await?;
        let group = user.group(database.clone(), None).await;

        //enforce the world limit
        if let Some(world_limit) = group.world_limit {
            if user_worlds.iter().count() >= world_limit as usize {
                return Err(DatabaseError::Unauthorized);
            }
        }

        json.hostname = into_valid_hostname(&json.hostname);
        if !database
            .get_all_where::<World, _>("hostname", json.hostname.clone(), None)
            .await?
            .is_empty()
        {
            debug!("hostname already used. adding a random value to it");
            json.hostname += &rand::random_range(0..100000).to_string();
        }

        //enforce memory limit
        if let Some(memory_limit) = group.total_memory_limit {
            if let Some(allocated_memory) = json.allocated_memory {
                let user_worlds: Vec<World> = database
                    .get_all_where("owner_id", user.id, Some((&user, &group)))
                    .await?;

                let mut total_memory = 0;

                for world in &user_worlds {
                    total_memory += world.allocated_memory;
                }
                let remaining_memory = memory_limit as i32 - total_memory as i32;
                if remaining_memory < 0 {
                    return Err(DatabaseError::Unauthorized);
                }

                if (allocated_memory as i32) > remaining_memory {
                    debug!(
                        "changing memory amount for server created by {}. requested is {}, max available is {}",
                        user.id, memory_limit, remaining_memory
                    );
                    json.allocated_memory = Some(remaining_memory.try_into().unwrap_or_default());
                }
            }
        }

        Ok(())
    }
    async fn after_api_create(
        &self,
        _database: Arc<Database>,
        _json: &mut Self::JsonFrom,
        _user: &User,
    ) -> Result<(), DatabaseError> {
        let _ = server::get_or_create_server(self).await;
        Ok(())
    }
}
#[async_trait]
impl ApiUpdate for World {
    //TODO: this needs a rewrite, too much repeated code
    async fn before_api_update(
        &self,
        database: Arc<Database>,
        json: &mut Self::JsonUpdate,
        user: &User,
    ) -> Result<(), DatabaseError> {
        let group = user.group(database.clone(), None).await;
        let user_worlds: Vec<World> = database
            .get_all_where("owner_id", user.id, Some((&user, &group)))
            .await?;

        //enforce the active world limit
        if let Some(active_world_limit) = group.active_world_limit {
            let mut active_worlds = 0;
            for world in &user_worlds {
                if world.enabled {
                    active_worlds += 1;
                }
            }
            if active_worlds >= active_world_limit {
                json.enabled = Some(false);
            }
        }

        //adjust hostname, so it's a valid, unique subdomain
        if let Some(hostname) = &json.hostname {
            json.hostname = Some(into_valid_hostname(hostname))
        }
        if let Some(hostname) = &json.hostname {
            if match database
                .get_where::<World, _>("hostname", hostname.clone(), None)
                .await
            {
                Ok(world) => {
                    if world.id == self.id {
                        false
                    } else {
                        true
                    }
                }
                Err(_) => false,
            } {
                debug!("hostname already used. adding a random value to it");
                json.hostname = Some(hostname.clone() + &rand::random_range(0..100000).to_string());
            }
        }

        //enforce memory limit
        if let Some(memory_limit) = group.total_memory_limit {
            if let Some(allocated_memory) = json.allocated_memory {
                if allocated_memory != self.allocated_memory as u32 {
                    let mut total_memory = 0;

                    for world in &user_worlds {
                        if world.id != self.id {
                            total_memory += world.allocated_memory;
                        }
                    }
                    let remaining_memory = memory_limit as i32 - total_memory as i32;
                    if remaining_memory < 0 {
                        return Err(DatabaseError::Unauthorized);
                    }

                    if (allocated_memory as i32) > remaining_memory {
                        debug!(
                            "changing memory amount for server {} created by {}. requested is {}, max available is {}",
                            self.id, user.id, memory_limit, remaining_memory
                        );
                        json.allocated_memory = Some(remaining_memory.try_into().unwrap());
                    }
                }
            }
        }

        Ok(())
    }
    async fn after_api_update(
        &self,
        _database: Arc<Database>,
        _json: &mut Self::JsonUpdate,
        _user: &User,
    ) -> Result<(), DatabaseError> {
        let server = server::get_or_create_server(self)
            .await
            .map_err(|err| DatabaseError::InternalServerError(err.to_string()))?;
        let mut server = server.lock().await;

        server
            .update_world(self.clone())
            .await
            .map_err(|err| DatabaseError::InternalServerError(err.to_string()))
    }
}
#[async_trait]
impl ApiRemove for World {
    async fn before_api_delete(
        &self,
        database: Arc<Database>,
        user: &User,
    ) -> Result<(), DatabaseError> {
        info!("removing world {}", self.id);
        match server::get_or_create_server(self).await {
            Ok(server) => server
                .lock()
                .await
                .remove()
                .await
                .map_err(|err| DatabaseError::InternalServerError(err.to_string())),
            Err(err) => Err(DatabaseError::InternalServerError(err.to_string())),
        }
    }
}

impl World {
    pub async fn version(&self, database: Arc<Database>, user: Option<(&User, &Group)>) -> Version {
        database
            .get_one(self.version_id, user)
            .await
            .expect(&format!(
                "couldn't find version with id {}",
                self.version_id
            ))
    }
    pub async fn owner(&self, database: Arc<Database>, user: Option<(&User, &Group)>) -> User {
        database
            .get_one(self.owner_id, user)
            .await
            .expect(&format!("couldn't find user with id {}", self.owner_id))
    }
    #[allow(clippy::needless_pass_by_value)]
    async fn world_get_status(
        _rate_limit_info: RateLimitInfo,
        id: String,
        db_mutex: Arc<Database>,
        user: User,
    ) -> Result<impl warp::Reply, warp::Rejection> {
        let id = Id::from_string(&id).map_err(|_| reject::custom(rejections::NotFound))?;
        {
            let group = user.group(db_mutex.clone(), None).await;
            db_mutex
                .get_one::<Self>(id, Some((&user, &group)))
                .await
                .map_err(crate::api::handlers::handle_database_error)?;
        }

        let server = server::get_server(id);
        let status = match server.await {
            Some(server) => server.lock().await.status().await,
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
        db_mutex: Arc<Database>,
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
            .and_then(Self::world_get_status)
    }

    #[allow(clippy::needless_pass_by_value)]
    async fn get_server_config(
        _rate_limit_info: RateLimitInfo,
        id: String,
        database: Arc<Database>,
        user: User,
    ) -> Result<impl warp::Reply, warp::Rejection> {
        let id = Id::from_string(&id).map_err(|_| reject::custom(rejections::NotFound))?;
        let group = user.group(database.clone(), None).await;

        let world = {
            database
                .get_one::<Self>(id, Some((&user, &group)))
                .await
                .map_err(crate::api::handlers::handle_database_error)?
        };

        let server = server::get_or_create_server(&world)
            .await
            .map_err(|err| warp::reject::custom(rejections::InternalServerError::from(err)))?;

        let server = server.lock().await;

        let mut config = server
            .config()
            .await
            .map_err(|err| warp::reject::custom(rejections::InternalServerError::from(err)))?;
        if group.config_whitelist.is_empty() {
            for key in group.config_blacklist {
                config.remove(&key);
            }
        } else {
            let mut new_config = HashMap::new();
            for key in group.config_whitelist {
                if let Some(value) = config.get(&key) {
                    new_config.insert(key, value.clone());
                }
            }
            config = new_config
        }

        Ok(warp::reply::with_status(
            warp::reply::json(&config),
            StatusCode::OK,
        ))
    }

    pub fn get_config_filter(
        db_mutex: Arc<Database>,
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
            .and_then(Self::get_server_config)
    }

    #[allow(clippy::needless_pass_by_value)]
    async fn set_server_config(
        _rate_limit_info: RateLimitInfo,
        id: String,
        database: Arc<Database>,
        user: User,
        new_config: HashMap<String, String>,
    ) -> Result<impl warp::Reply, warp::Rejection> {
        let id = Id::from_string(&id).map_err(|_| reject::custom(rejections::NotFound))?;
        let group = user.group(database.clone(), None).await;

        let world = {
            database
                .get_one::<Self>(id, Some((&user, &group)))
                .await
                .map_err(crate::api::handlers::handle_database_error)?
        };

        let server = server::get_or_create_server(&world)
            .await
            .map_err(|err| warp::reject::custom(rejections::InternalServerError::from(err)))?;

        let mut server = server.lock().await;

        let mut config = server
            .config()
            .await
            .map_err(|err| warp::reject::custom(rejections::InternalServerError::from(err)))?;
        if group.config_whitelist.is_empty() {
            for key in &group.config_blacklist {
                config.remove(key.as_str());
            }
        } else {
            let mut new_config = HashMap::new();
            for key in &group.config_whitelist {
                if let Some(value) = config.get(key.as_str()) {
                    new_config.insert(key.clone(), value.clone());
                }
            }
            config = new_config
        }

        for (key, value) in new_config {
            let mut editable = true;
            if group.config_whitelist.is_empty() {
                if group.config_blacklist.contains(&key) {
                    editable = false;
                }
            } else if !group.config_whitelist.contains(&key) {
                editable = false;
            }

            if editable {
                if let Some(config_limit) = group.config_limits.get(&key) {
                    match config_limit {
                        ServerConfigLimit::MoreThan(limit) | ServerConfigLimit::LessThan(limit) => {
                            if let Ok(value) = value.parse::<i64>() {
                                let over_limit = match config_limit {
                                    ServerConfigLimit::MoreThan(limit) => value < *limit,
                                    ServerConfigLimit::LessThan(limit) => value > *limit,
                                    _ => true, //huh
                                };
                                if over_limit {
                                    config.insert(key, limit.to_string());
                                } else {
                                    config.insert(key, value.to_string());
                                }
                            }
                            //if the value is invalid don't set it
                        }
                        ServerConfigLimit::Whitelist(whitelist) => {
                            if whitelist.contains(&value) {
                                config.insert(key, value);
                            }
                        }
                    }
                } else {
                    config.insert(key, value);
                }
            }
        }

        server
            .set_config(config.clone())
            .await
            .map_err(|err| warp::reject::custom(rejections::InternalServerError::from(err)))?;

        Ok(warp::reply::with_status(
            warp::reply::json(&config),
            StatusCode::OK,
        ))
    }
    pub fn set_config_filter(
        database: Arc<Database>,
        rate_limit_config: RateLimitConfig,
    ) -> impl Filter<Extract = (impl warp::Reply,), Error = warp::Rejection> + Clone {
        warp::path("api")
            .and(warp_rate_limit::with_rate_limit(rate_limit_config))
            .and(warp::path(Self::table_name()))
            .and(warp::path::param::<String>())
            .and(warp::path("config"))
            .and(warp::path::end())
            .and(warp::put())
            .and(filters::with_db(database.clone()))
            .and(filters::with_auth(database))
            .and(warp::filters::body::json())
            .and_then(Self::set_server_config)
    }
}
