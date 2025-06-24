use std::collections::HashMap;
use rusqlite::{Row, ToSql};
use rusqlite::types::ToSqlOutput;
use serde::{Deserialize, Deserializer, Serialize};
use warp::{Filter, Rejection, Reply};
use warp_rate_limit::RateLimitConfig;
use crate::api::handlers::{ApiCreate, ApiGet, ApiList, ApiObject, ApiRemove, ApiUpdate, DbMutex};
use crate::database::objects::{DbObject, FromJson, InviteLink, UpdateJson, User};
use crate::database::types::{Access, Column, Id, Type};
use crate::minecraft::server::ServerConfigLimit;

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct Group {
    /// group's unique [`Id`]
    pub id: Id,
    /// group's name
    pub name: String,
    /// limit of user's total allocatable memory in MiB. [`None`] means no limit
    pub total_memory_limit: Option<u32>,
    /// limit of user's per-world allocatable memory in MiB. [`None`] means no limit
    pub per_world_memory_limit: Option<u32>,
    /// how many worlds can a user create. [`None`] means no limit
    pub world_limit: Option<u32>,
    /// how many worlds can be enabled at a time. [`None`] means no limit
    pub active_world_limit: Option<u32>,
    /// how much storage is available to a user in MiB. [`None`] means no limit
    pub storage_limit: Option<u32>,
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
            Column::new("id", Type::Id).primary_key(),
            Column::new("name", Type::Text).not_null(),
            Column::new("total_memory_limit", Type::Integer(false)),
            Column::new("per_world_memory_limit", Type::Integer(false)),
            Column::new("world_limit", Type::Integer(false)),
            Column::new("active_world_limit", Type::Integer(false)),
            Column::new("storage_limit", Type::Integer(false)),
            Column::new("config_blacklist", Type::Text),
            Column::new("config_whitelist", Type::Text),
            Column::new("config_limits", Type::Text),
            Column::new("is_privileged", Type::Boolean)
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
            name: row.get(1)?,
            total_memory_limit: row.get(2)?,
            per_world_memory_limit: row.get(3)?,
            world_limit: row.get(4)?,
            active_world_limit: row.get(5)?,
            storage_limit: row.get(6)?,
            // yes. I am storing JSON in a database. no. you cannot stop me.
            config_blacklist: serde_json::from_str(&row.get::<usize, String>(7)?)
                .map_err(|_| rusqlite::Error::UnwindingPanic)?,
            config_whitelist: serde_json::from_str(&row.get::<usize, String>(8)?)
                .map_err(|_| rusqlite::Error::UnwindingPanic)?,
            config_limits: serde_json::from_str(&row.get::<usize, String>(9)?)
                .map_err(|_| rusqlite::Error::UnwindingPanic)?,
            is_privileged: row.get(10)?,
        })
    }

    fn get_id(&self) -> Id {
        self.id
    }

    fn params(&self) -> Vec<ToSqlOutput> {
        let config_blacklist =
            serde_json::to_string(&self.config_blacklist).expect("serialization failed");
        let config_whitelist =
            serde_json::to_string(&self.config_whitelist).expect("serialization failed");
        let config_limits =
            serde_json::to_string(&self.config_limits).expect("serialization failed");
        vec![
            self.id
                .to_sql()
                .expect("failed to convert the value to sql"),
            self.name.to_sql().expect("failed to convert value to sql"),
            self.total_memory_limit
                .to_sql()
                .expect("failed to convert the value to sql"),
            self.per_world_memory_limit
                .to_sql()
                .expect("failed to convert the value to sql"),
            self.world_limit
                .to_sql()
                .expect("failed to convert the value to sql"),
            self.active_world_limit
                .to_sql()
                .expect("failed to convert the value to sql"),
            self.storage_limit
                .to_sql()
                .expect("failed to convert the value to sql"),
            ToSqlOutput::from(config_blacklist),
            ToSqlOutput::from(config_whitelist),
            ToSqlOutput::from(config_limits),
            self.is_privileged
                .to_sql()
                .expect("failed to convert the value to sql"),
        ]
    }
}

impl ApiObject for Group {
    fn filters(
        db_mutex: DbMutex,
        rate_limit_config: RateLimitConfig,
    ) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone {
        Self::list_filter(db_mutex.clone(), rate_limit_config.clone())
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

#[derive(Debug, Clone, Deserialize)]
pub struct JsonFrom {
    pub name: String,
    pub total_memory_limit: Option<u32>,
    pub per_world_memory_limit: Option<u32>,
    pub world_limit: Option<u32>,
    pub active_world_limit: Option<u32>,
    pub storage_limit: Option<u32>,
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

// Any value that is present is considered Some value, including null.
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
    pub total_memory_limit: Option<Option<u32>>,
    #[serde(default, deserialize_with = "deserialize_some")]
    pub per_world_memory_limit: Option<Option<u32>>,
    #[serde(default, deserialize_with = "deserialize_some")]
    pub world_limit: Option<Option<u32>>,
    #[serde(default, deserialize_with = "deserialize_some")]
    pub active_world_limit: Option<Option<u32>>,
    #[serde(default, deserialize_with = "deserialize_some")]
    pub storage_limit: Option<Option<u32>>,
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
