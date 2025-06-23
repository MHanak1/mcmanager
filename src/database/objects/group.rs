use std::collections::HashMap;
use rusqlite::{Row, ToSql};
use rusqlite::types::ToSqlOutput;
use serde::{Deserialize, Serialize};
use crate::database::objects::DbObject;
use crate::database::types::{Access, Column, Id, Type};
use crate::minecraft::server::ServerConfigLimit;

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct Group {
    /// group's unique [`Id`]
    pub id: Id,
    /// group's unique name
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
            Column::new("memory_limit", Type::Integer(false)),
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
