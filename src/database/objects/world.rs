use crate::api::handlers::{ApiCreate, ApiGet, ApiList, ApiRemove, ApiUpdate};
use crate::database::objects::{DbObject, FromJson, UpdateJson, User};
use crate::database::types::{Access, Column, Id, Type};
use rusqlite::types::ToSqlOutput;
use rusqlite::{Row, ToSql};
use serde::{Deserialize, Deserializer, Serialize};

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
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
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

impl ApiList for World {}
impl ApiGet for World {}
impl ApiCreate for World {}
impl ApiUpdate for World {}
impl ApiRemove for World {}
