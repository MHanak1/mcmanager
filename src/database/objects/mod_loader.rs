use crate::api::handlers::{ApiCreate, ApiGet, ApiList, ApiRemove, ApiUpdate};
use crate::database::objects::{DbObject, FromJson, UpdateJson, User};
use crate::database::types::{Access, Column, Id, Type};
use rusqlite::types::ToSqlOutput;
use rusqlite::{Row, ToSql};
use serde::{Deserialize, Serialize};
use std::fmt::Debug;

/// `id`: the mod loader's unique [`Id`]
///
/// `name`: mod loader's name (like "Fabric" or "Vanilla")
///
/// `can_load_mods`: if the mod loader actually can load mods
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct ModLoader {
    pub id: Id,
    pub name: String,
    pub can_load_mods: bool,
}

impl DbObject for ModLoader {
    fn view_access() -> Access {
        Access::User
    }

    fn update_access() -> Access {
        Access::PrivilegedUser
    }

    fn create_access() -> Access {
        Access::PrivilegedUser
    }

    fn table_name() -> &'static str {
        "mod_loaders"
    }

    fn columns() -> Vec<Column> {
        vec![
            Column::new("id", Type::Id).primary_key(),
            Column::new("name", Type::Text).not_null(),
            Column::new("can_load_mods", Type::Boolean)
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
            can_load_mods: row.get(2)?,
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
            self.name
                .to_sql()
                .expect("failed to convert the value to sql"),
            self.can_load_mods
                .to_sql()
                .expect("failed to convert the value to sql"),
        ]
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct JsonFrom {
    pub name: String,
    pub can_load_mods: bool,
}

impl FromJson for ModLoader {
    type JsonFrom = JsonFrom;

    fn from_json(data: &Self::JsonFrom, _user: &User) -> Self {
        Self {
            id: Id::default(),
            name: data.name.clone(),
            can_load_mods: data.can_load_mods,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct JsonUpdate {
    pub name: Option<String>,
    pub can_load_mods: Option<bool>,
}

impl UpdateJson for ModLoader {
    type JsonUpdate = JsonUpdate;
    fn update_with_json(&self, data: &Self::JsonUpdate) -> Self {
        let mut new = self.clone();
        new.name = data.name.clone().unwrap_or(new.name);
        new.can_load_mods = data.can_load_mods.unwrap_or(new.can_load_mods);
        new
    }
}

impl ApiList for ModLoader {}
impl ApiGet for ModLoader {}
impl ApiCreate for ModLoader {}
impl ApiUpdate for ModLoader {}
impl ApiRemove for ModLoader {}
