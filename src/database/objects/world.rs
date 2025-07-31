use crate::api::filters;
use crate::api::filters::UserAuth;
use crate::api::handlers::{ApiCreate, ApiGet, ApiIcon, ApiList, ApiObject, ApiRemove, ApiUpdate};
use crate::api::serve::AppState;
use crate::config::CONFIG;
use crate::database::objects::group::Group;
use crate::database::objects::{DbObject, FromJson, ModLoader, UpdateJson, User, Version};
use crate::database::types::{Access, Column, Id};
use crate::database::{Cachable, Database, DatabaseError, ValueType};
use crate::minecraft::server;
use crate::minecraft::server::{MinecraftServerStatus, ServerConfigLimit};
use async_trait::async_trait;
use axum::Router;
use axum::extract::{Path, State, WebSocketUpgrade};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use log::{debug, error, info};
use once_cell::sync::Lazy;
use serde::{Deserialize, Deserializer, Serialize};
use sqlx::{Any, Arguments, Encode, FromRow, IntoArguments};
use std::collections::HashMap;
use std::os::linux::raw::stat;
use std::sync::Arc;
use serde_json::json;
use socketioxide::extract::SocketRef;
use tokio::sync::Mutex;
use tokio::task::id;

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

    const COLUMNS: Lazy<Vec<Column>> = Lazy::new(|| {
        vec![
            Column::new("id", ValueType::Id).primary_key(),
            Column::new("owner_id", ValueType::Id)
                .not_null()
                .references("users(id)"),
            Column::new("name", ValueType::Text).not_null(),
            Column::new("hostname", ValueType::Text).not_null().unique(),
            Column::new("allocated_memory", ValueType::Integer).not_null(),
            Column::new("version_id", ValueType::Id)
                .not_null()
                .references("versions(id)"),
            Column::new("enabled", ValueType::Boolean)
                .not_null()
                .default("false"),
        ]
    });

    fn id(&self) -> Id {
        self.id
    }

    fn owner_id(&self) -> Option<Id> {
        Some(self.owner_id)
    }
}

impl Cachable for World {
    fn into_any(self: Box<Self>) -> Box<dyn std::any::Any> {
        self as Box<dyn std::any::Any>
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
    fn routes() -> Router<AppState> {
        Router::new()
            .route("/", get(Self::api_list).post(Self::api_create))
            .route(
                "/{id}",
                get(Self::api_get)
                    .patch(Self::api_update)
                    .delete(Self::api_remove),
            )
            .route(
                "/{id}/config",
                get(Self::get_server_config)
                    .patch(Self::set_server_config)
                    .post(Self::set_server_config),
            )
            .route("/{id}/status", get(Self::world_get_status))
            .route(
                "/{id}/log",
                    get(Self::get_server_log)
            )
            .route(
                "/{id}/icon",
                post(Self::upload_icon)
                    .patch(Self::upload_icon)
                    .get(Self::get_icon)
            )
            .route(
                "/default/icon",
                get(Self::default_icon)
            )
    }
}

impl ApiList for World {}
impl ApiGet for World {}
#[async_trait]
impl ApiCreate for World {
    //TODO: this needs a rewrite, too much repeated code
    async fn before_api_create(
        state: AppState,
        json: &mut Self::JsonFrom,
        user: &User,
    ) -> Result<(), DatabaseError> {
        let group = user.group(state.database.clone(), None).await;
        let user_worlds: Vec<World> = state
            .database
            .get_all_where("owner_id", user.id, Some((&user, &group)))
            .await?;
        let group = user.group(state.database.clone(), None).await;

        //enforce the world limit
        if let Some(world_limit) = group.world_limit {
            if user_worlds.iter().count() >= world_limit as usize {
                return Err(DatabaseError::Unauthorized);
            }
        }

        json.hostname = into_valid_hostname(&json.hostname);
        if !state
            .database
            .get_all_where::<World, _>("hostname", json.hostname.clone(), None)
            .await?
            .is_empty()
        {
            debug!("hostname already used. adding a random value to it");
            json.hostname += &rand::random_range(0..100000).to_string();
        }

        json.allocated_memory = Some(
            json.allocated_memory
                .unwrap_or(CONFIG.world_defaults.allocated_memory),
        );

        //enforce memory limit
        if let Some(mut allocated_memory) = json.allocated_memory {
            if allocated_memory < CONFIG.world.minimum_memory {
                return Err(DatabaseError::Unauthorized);
            }
            if let Some(group_mem_limit) = group.per_world_memory_limit {
                if allocated_memory as u64 > group_mem_limit as u64 {
                    return Err(DatabaseError::Unauthorized);
                }
            }

            //do not enforce total memory limit, as the world will not be enabled yet
        }

        Ok(())
    }
    async fn after_api_create(
        &self,
        appstate: AppState,
        _json: &mut Self::JsonFrom,
        _user: &User,
    ) -> Result<(), DatabaseError> {
        let _ = appstate.servers.get_or_create_server(self).await;
        Ok(())
    }
}
#[async_trait]
impl ApiUpdate for World {
    //TODO: this needs a rewrite, too much repeated code
    async fn before_api_update(
        &self,
        state: AppState,
        json: &mut Self::JsonUpdate,
        user: &User,
    ) -> Result<(), DatabaseError> {
        let group = user.group(state.database.clone(), None).await;
        let user_worlds: Vec<World> = state
            .database
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
            if match state
                .database
                .get_where::<World, _>("hostname", hostname.clone(), None)
                .await
            {
                Ok(world) => world.id != self.id,
                Err(_) => false,
            } {
                debug!("hostname already used. adding a random value to it");
                json.hostname = Some(hostname.clone() + &rand::random_range(0..100000).to_string());
            }
        }

        json.allocated_memory = Some(
            json.allocated_memory
                .unwrap_or(self.allocated_memory as u32),
        );
        let allocated_memory = json.allocated_memory.unwrap();
        let enabled = json.enabled.unwrap_or(self.enabled);

        //enforce memory limit
        if enabled {
            if allocated_memory < CONFIG.world.minimum_memory {
                return Err(DatabaseError::Unauthorized);
            }
            if let Some(group_mem_limit) = group.per_world_memory_limit {
                if allocated_memory as u64 > group_mem_limit as u64 {
                    return Err(DatabaseError::Unauthorized);
                }
            }

            if let Some(memory_limit) = group.total_memory_limit {
                if allocated_memory != self.allocated_memory as u32 {
                    // sum of total users memory usage plus the new usage
                    let mut total_memory = user.total_memory_usage + allocated_memory as i64;

                    //if the world was previously enabled, it means it contributed to total allocated memory, and we don't want to include it
                    if self.enabled {
                        total_memory -= self.allocated_memory as i64;
                    }

                    if total_memory > memory_limit as i64 {
                        return Err(DatabaseError::Unauthorized);
                    }
                }
            }
        }

        Ok(())
    }
    async fn after_api_update(
        &self,
        app_state: AppState,
        _json: &mut Self::JsonUpdate,
        user: &User,
    ) -> Result<(), DatabaseError> {
        let server = app_state
            .servers
            .get_or_create_server(self)
            .await
            .map_err(|err| DatabaseError::InternalServerError(err.to_string()))?;
        let mut server = server.lock().await;

        server
            .update_world(self.clone())
            .await
            .map_err(|err| DatabaseError::InternalServerError(err.to_string()))?;

        let user_enabled_worlds = app_state
            .database
            .get_all_where::<World, _>(
                "enabled",
                true,
                Some((user, &user.group(app_state.database.clone(), None).await)),
            )
            .await?;

        let mut total_memory_usage = 0;
        for world in &user_enabled_worlds {
            total_memory_usage += world.allocated_memory;
        }
        let mut user = user.clone();

        user.total_memory_usage = total_memory_usage as i64;

        app_state.database.update(&user, None).await?;

        Ok(())
    }
}
#[async_trait]
impl ApiRemove for World {
    async fn before_api_delete(
        &self,
        app_state: AppState,
        user: &User,
    ) -> Result<(), DatabaseError> {
        info!("removing world {}", self.id);
        match app_state.servers.get_or_create_server(self).await {
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

impl ApiIcon for World {
    const DEFAULT_ICON_BYTES: &'static [u8] = include_bytes!("../../resources/icons/world_default.png");
    const DEFAULT_ICON_MIME: &'static str = "image/png";
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MinecraftServerStatusJson {
    pub(crate) status: String,
    pub(crate) code: u32,
}

impl World {
    pub async fn version(&self, database: Database, user: Option<(&User, &Group)>) -> Version {
        database
            .get_one(self.version_id, user)
            .await
            .expect(&format!(
                "couldn't find version with id {}",
                self.version_id
            ))
    }
    pub async fn owner(&self, database: Database, user: Option<(&User, &Group)>) -> User {
        database
            .get_one::<User>(self.owner_id, user)
            .await
            .expect(&format!("couldn't find user with id {}", self.owner_id))
    }

    #[allow(clippy::needless_pass_by_value)]
    async fn world_get_status(
        id: Path<Id>,
        state: State<AppState>,
        user: UserAuth,
    ) -> Result<impl IntoResponse, axum::http::StatusCode> {
        let id = id.0;
        let state = state.0;
        let user = user.0;

        {
            let group = user.group(state.database.clone(), None).await;
            state
                .database
                .get_one::<Self>(id, Some((&user, &group)))
                .await
                .map_err(crate::api::handlers::handle_database_error)?;
        }

        let server = state.servers.get_server(id);
        let status = match server {
            Some(server) => server.lock().await.status().await,
            None => Ok(MinecraftServerStatus::Exited(0)),
        }
        .map_err(|err| {
            error!("{err}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        Ok(axum::Json(MinecraftServerStatusJson::from(status)))
    }

    #[allow(clippy::needless_pass_by_value)]
    async fn get_server_config(
        id: Path<Id>,
        state: State<AppState>,
        user: UserAuth,
    ) -> Result<impl IntoResponse, axum::http::StatusCode> {
        let id = id.0;
        let state = state.0;
        let user = user.0;

        let group = user.group(state.database.clone(), None).await;

        let world = {
            state
                .database
                .get_one::<Self>(id, Some((&user, &group)))
                .await
                .map_err(crate::api::handlers::handle_database_error)?
        };

        let server = state
            .servers
            .get_or_create_server(&world)
            .await
            .map_err(|err| {
                error!("{err}");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

        let server = server.lock().await;

        let mut config = server.config().await.map_err(|err| {
            error!("{err}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
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

        Ok(axum::Json(config))
    }

    #[allow(clippy::needless_pass_by_value)]
    async fn set_server_config(
        id: Path<Id>,
        state: State<AppState>,
        user: UserAuth,
        new_config: axum::extract::Json<HashMap<String, String>>,
    ) -> Result<impl IntoResponse, axum::http::StatusCode> {
        let id = id.0;
        let state = state.0;
        let user = user.0;
        let new_config = new_config.0;

        let group = user.group(state.database.clone(), None).await;

        let world = {
            state
                .database
                .get_one::<Self>(id, Some((&user, &group)))
                .await
                .map_err(crate::api::handlers::handle_database_error)?
        };

        let server = state
            .servers
            .get_or_create_server(&world)
            .await
            .map_err(|err| {
                error!("{err}");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

        let mut server = server.lock().await;

        let mut config = server.config().await.map_err(|err| {
            error!("{err}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
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

        server.set_config(config.clone()).await.map_err(|err| {
            error!("{err}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        Ok(axum::Json(config))
    }

    async fn get_server_log(
        id: Path<Id>,
        state: State<AppState>,
        user: UserAuth,
    ) -> Result<impl IntoResponse, axum::http::StatusCode> {
        let id = id.0;
        let state = state.0;
        let user = user.0;

        let group = user.group(state.database.clone(), None).await;

        let world = {
            state
                .database
                .get_one::<Self>(id, Some((&user, &group)))
                .await
                .map_err(crate::api::handlers::handle_database_error)?
        };

        let server = state
            .servers
            .get_or_create_server(&world)
            .await
            .map_err(|err| {
                error!("{err}");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

        let mut server = server.lock().await;

        Ok(axum::Json(json!({"log": server.latest_log().await.unwrap_or_default()})))

    }

    async fn test() {}
}
