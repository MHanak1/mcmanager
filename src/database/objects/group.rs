use crate::api::handlers::{ApiCreate, ApiGet, ApiList, ApiObject, ApiRemove, ApiUpdate};
use crate::database::objects::{DbObject, FromJson, InviteLink, UpdateJson, User};
use crate::database::types::{Access, Column, Id, ValueType};
use crate::minecraft::server::ServerConfigLimit;
use serde::{Deserialize, Deserializer, Serialize};
use sqlx::{Arguments, Encode, Error, FromRow, IntoArguments, Row};
use std::collections::HashMap;
use std::sync::Arc;
use warp::{Filter, Rejection, Reply};
use warp_rate_limit::RateLimitConfig;
use crate::database::{Database, DatabaseType};

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct Group {
    /// group's unique [`Id`]
    pub id: Id,
    /// group's name
    pub name: String,
    /// limit of user's total allocatable memory in MiB. [`None`] means no limit
    pub total_memory_limit: Option<i32>,
    /// limit of user's per-world allocatable memory in MiB. [`None`] means no limit
    pub per_world_memory_limit: Option<i32>,
    /// how many worlds can a user create. [`None`] means no limit
    pub world_limit: Option<i32>,
    /// how many worlds can be enabled at a time. [`None`] means no limit
    pub active_world_limit: Option<i32>,
    /// how much storage is available to a user in MiB. [`None`] means no limit
    pub storage_limit: Option<i32>,
    /// server.properties config limitation. for more info look at the description in the config file
    pub config_blacklist: Vec<String>,
    /// server.properties config limitation. for more info look at the description in the config file
    pub config_whitelist: Vec<String>,
    /// server.properties config limitation. for more info look at the description in the config file
    pub config_limits: HashMap<String, ServerConfigLimit>,
    /// whether a user has administrative privileges, this means they can manage other users and create new accounts
    pub is_privileged: bool,
}

impl DbObject for Group {
    fn view_access() -> Access {
        Access::All
    }

    fn update_access() -> Access {
        Access::PrivilegedUser
    }

    fn create_access() -> Access {
        Access::PrivilegedUser
    }

    fn table_name() -> &'static str {
        "groups"
    }

    fn columns() -> Vec<Column> {
        vec![
            Column::new("id", ValueType::Id).primary_key(),
            Column::new("name", ValueType::Text).not_null(),
            Column::new("total_memory_limit", ValueType::Integer(false)),
            Column::new("per_world_memory_limit", ValueType::Integer(false)),
            Column::new("world_limit", ValueType::Integer(false)),
            Column::new("active_world_limit", ValueType::Integer(false)),
            Column::new("storage_limit", ValueType::Integer(false)),
            Column::new("config_blacklist", ValueType::Text),
            Column::new("config_whitelist", ValueType::Text),
            Column::new("config_limits", ValueType::Text),
            Column::new("is_privileged", ValueType::Boolean)
                .not_null()
                .default("false"),
        ]
    }

    fn get_id(&self) -> Id {
        self.id
    }
}

impl<'a> FromRow<'_, <DatabaseType as sqlx::Database>::Row> for Group {
    fn from_row(row: &'_ <DatabaseType as sqlx::Database>::Row) -> Result<Self, Error> {
        Ok(Self {
            id: row.try_get("id")?,
            name: row.try_get("name")?,
            total_memory_limit: row.try_get("total_memory_limit")?,
            per_world_memory_limit: row.try_get("per_world_memory_limit")?,
            world_limit: row.try_get("world_limit")?,
            active_world_limit: row.try_get("active_world_limit")?,
            storage_limit: row.try_get("storage_limit")?,
            config_blacklist: serde_json::from_str(&row.try_get::<String, _>("config_blacklist")?)
                .unwrap(),
            config_whitelist: serde_json::from_str(&row.try_get::<String, _>("config_whitelist")?)
                .unwrap(),
            config_limits: serde_json::from_str(&row.try_get::<String, _>("config_limits")?)
                .unwrap(),
            is_privileged: row.try_get("is_privileged")?,
        })
    }
}

impl<'a> IntoArguments<'a, crate::database::DatabaseType> for Group {
    fn into_arguments(self) -> <crate::database::DatabaseType as sqlx::Database>::Arguments<'a> {
        let config_blacklist =
            serde_json::to_string(&self.config_blacklist).expect("serialization failed");
        let config_whitelist =
            serde_json::to_string(&self.config_whitelist).expect("serialization failed");
        let config_limits =
            serde_json::to_string(&self.config_limits).expect("serialization failed");

        let mut arguments = <crate::database::DatabaseType as sqlx::Database>::Arguments::default();

        arguments.add(self.id).expect("Failed to add argument");
        arguments.add(self.name).expect("Failed to argument");
        arguments
            .add(self.total_memory_limit)
            .expect("Failed to add argument");
        arguments
            .add(self.per_world_memory_limit)
            .expect("Failed to add argument");
        arguments
            .add(self.world_limit)
            .expect("Failed to add argument");
        arguments
            .add(self.active_world_limit)
            .expect("Failed to add argument");
        arguments
            .add(self.storage_limit)
            .expect("Failed to add argument");
        arguments
            .add(config_blacklist)
            .expect("Failed to add argument");
        arguments
            .add(config_whitelist)
            .expect("Failed to add argument");
        arguments
            .add(config_limits)
            .expect("Failed to add argument");
        arguments
            .add(self.is_privileged)
            .expect("Failed to add argument");
        arguments
    }
}

impl ApiObject for Group {
    fn filters (
        database: Arc<Database>,
        rate_limit_config: RateLimitConfig,
    ) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone {
        Self::list_filter(database.clone(), rate_limit_config.clone())
            .or(Self::get_filter(
                database.clone(),
                rate_limit_config.clone(),
            ))
            .or(Self::create_filter(
                database.clone(),
                rate_limit_config.clone(),
            ))
            .or(Self::update_filter(
                database.clone(),
                rate_limit_config.clone(),
            ))
            .or(Self::remove_filter(
                database,
                rate_limit_config.clone(),
            ))
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct JsonFrom {
    pub name: String,
    pub total_memory_limit: Option<i32>,
    pub per_world_memory_limit: Option<i32>,
    pub world_limit: Option<i32>,
    pub active_world_limit: Option<i32>,
    pub storage_limit: Option<i32>,
    pub config_blacklist: Option<Vec<String>>,
    pub config_whitelist: Option<Vec<String>>,
    pub config_limits: Option<HashMap<String, ServerConfigLimit>>,
    pub is_privileged: Option<bool>,
}

impl FromJson for Group {
    type JsonFrom = JsonFrom;
    fn from_json(data: &Self::JsonFrom, _user: &User) -> Self {
        Self {
            id: Id::default(),
            name: data.name.clone(),
            total_memory_limit: data.total_memory_limit,
            per_world_memory_limit: data.per_world_memory_limit,
            world_limit: data.world_limit,
            active_world_limit: data.active_world_limit,
            storage_limit: data.storage_limit,
            config_blacklist: data.config_blacklist.clone().unwrap_or_default(),
            config_whitelist: data.config_whitelist.clone().unwrap_or_default(),
            config_limits: data.config_limits.clone().unwrap_or_default(),
            is_privileged: data.is_privileged.unwrap_or(false),
        }
    }
}

// crate::database::DatabaseType value that is present is considered Some value, including null.
fn deserialize_some<'de, T, D>(deserializer: D) -> Result<Option<T>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    Deserialize::deserialize(deserializer).map(Some)
}

#[derive(Debug, Clone, Deserialize)]
pub struct JsonUpdate {
    #[serde(default, deserialize_with = "deserialize_some")]
    pub name: Option<String>,
    #[serde(default, deserialize_with = "deserialize_some")]
    pub avatar_id: Option<Option<Id>>,
    #[serde(default, deserialize_with = "deserialize_some")]
    pub total_memory_limit: Option<Option<i32>>,
    #[serde(default, deserialize_with = "deserialize_some")]
    pub per_world_memory_limit: Option<Option<i32>>,
    #[serde(default, deserialize_with = "deserialize_some")]
    pub world_limit: Option<Option<i32>>,
    #[serde(default, deserialize_with = "deserialize_some")]
    pub active_world_limit: Option<Option<i32>>,
    #[serde(default, deserialize_with = "deserialize_some")]
    pub storage_limit: Option<Option<i32>>,
    #[serde(default, deserialize_with = "deserialize_some")]
    pub config_blacklist: Option<Vec<String>>,
    #[serde(default, deserialize_with = "deserialize_some")]
    pub config_whitelist: Option<Vec<String>>,
    #[serde(default, deserialize_with = "deserialize_some")]
    pub config_limits: Option<HashMap<String, ServerConfigLimit>>,
    #[serde(default, deserialize_with = "deserialize_some")]
    pub is_privileged: Option<bool>,
}
impl UpdateJson for Group {
    type JsonUpdate = JsonUpdate;
    fn update_with_json(&self, data: &Self::JsonUpdate) -> Self {
        let mut new = self.clone();
        new.name = data.name.clone().unwrap_or(new.name);
        new.total_memory_limit = data.total_memory_limit.unwrap_or(new.total_memory_limit);
        new.per_world_memory_limit = data
            .per_world_memory_limit
            .unwrap_or(new.per_world_memory_limit);
        new.world_limit = data.world_limit.unwrap_or(new.world_limit);
        new.active_world_limit = data.active_world_limit.unwrap_or(new.active_world_limit);
        new.storage_limit = data.storage_limit.unwrap_or(new.storage_limit);
        new.config_blacklist = data
            .config_blacklist
            .clone()
            .unwrap_or(new.config_blacklist);
        new.config_whitelist = data
            .config_whitelist
            .clone()
            .unwrap_or(new.config_whitelist);
        new.config_limits = data.config_limits.clone().unwrap_or(new.config_limits);
        new.is_privileged = data.is_privileged.unwrap_or(new.is_privileged);
        new
    }
}

impl ApiList for Group {}
impl ApiGet for Group {}
impl ApiCreate for Group {}
impl ApiUpdate for Group {}
impl ApiRemove for Group {}
