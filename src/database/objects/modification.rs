use crate::api::handlers::{ApiCreate, ApiGet, ApiList, ApiObject, ApiRemove, ApiUpdate};
use crate::database::Database;
use crate::database::objects::{DbObject, FromJson, UpdateJson, User};
use crate::database::types::{Access, Column, Id, Type};
use rusqlite::types::ToSqlOutput;
use rusqlite::{Row, ToSql};
use serde::{Deserialize, Deserializer, Serialize};
use std::fmt::Debug;
use std::sync::{Arc, Mutex};
use warp::{Filter, Rejection, Reply};
use warp_rate_limit::RateLimitConfig;

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct Mod {
    /// Mod's unique [`Id`]
    pub id: Id,
    /// [`User`] who created nad owns the mod
    pub owner_id: Id,
    /// Minecraft's [`Version`] the mod is compatible with
    pub version_id: Id,
    /// Name displayed to the client
    pub name: String,
    /// Mod's description
    pub description: String,
    /// [`Id`] of the icon stored in the filesystem (data/icons)
    pub icon_id: Option<Id>,
}

impl DbObject for Mod {
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
        "mods"
    }

    fn columns() -> Vec<Column> {
        vec![
            Column::new("id", Type::Id).primary_key(),
            Column::new("owner_id", Type::Id)
                .not_null()
                .references("users(id)"),
            Column::new("version_id", Type::Id)
                .not_null()
                .references("versions(id)"),
            Column::new("name", Type::Text).not_null(),
            Column::new("description", Type::Text).not_null(),
            Column::new("icon_id", Type::Id),
        ]
    }

    fn from_row(row: &Row) -> rusqlite::Result<Self>
    where
        Self: Sized,
    {
        Ok(Self {
            id: row.get(0)?,
            owner_id: row.get(1)?,
            version_id: row.get(2)?,
            name: row.get(3)?,
            description: row.get(4)?,
            icon_id: row.get(5)?,
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
            self.version_id
                .to_sql()
                .expect("failed to convert the value to sql"),
            self.name
                .to_sql()
                .expect("failed to convert the value to sql"),
            self.description
                .to_sql()
                .expect("failed to convert the value to sql"),
            self.icon_id
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
    pub version_id: Id,
    pub name: String,
    pub description: Option<String>,
    pub icon_id: Option<Id>,
}

impl FromJson for Mod {
    type JsonFrom = JsonFrom;

    fn from_json(data: &Self::JsonFrom, user: &User) -> Self {
        Self {
            id: Id::default(),
            version_id: data.version_id,
            name: data.name.clone(),
            description: data.description.clone().unwrap_or_default(),
            icon_id: data.icon_id,
            owner_id: user.id,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct JsonUpdate {
    pub version_id: Option<Id>,
    pub name: Option<String>,
    pub description: Option<String>,
    #[serde(default, deserialize_with = "deserialize_some")]
    pub icon_id: Option<Option<Id>>,
}

impl UpdateJson for Mod {
    type JsonUpdate = JsonUpdate;
    fn update_with_json(&self, data: &Self::JsonUpdate) -> Self {
        let mut new = self.clone();
        new.version_id = data.version_id.unwrap_or(new.version_id);
        new.description = data.description.clone().unwrap_or(new.description);
        new.name = data.name.clone().unwrap_or(new.name);
        new.icon_id = data.icon_id.unwrap_or(new.icon_id);
        new
    }
}

impl ApiObject for Mod {
    fn filters(
        db_mutex: Arc<Mutex<Database>>,
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

impl ApiList for Mod {}
impl ApiGet for Mod {}
impl ApiCreate for Mod {}
impl ApiUpdate for Mod {}
impl ApiRemove for Mod {}
