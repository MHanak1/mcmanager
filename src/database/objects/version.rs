use crate::api::handlers::{ApiCreate, ApiGet, ApiList, ApiRemove, ApiUpdate};
use crate::database::objects::{DbObject, FromJson, UpdateJson, User};
use crate::database::types::{Access, Column, Id, Type};
use rusqlite::types::ToSqlOutput;
use rusqlite::{Row, ToSql};
use serde::{Deserialize, Serialize};

/// `id`: version's unique [`Id`]
///
/// `minecraft_version`: version string displayed to the client (like "1.20.1")
///
/// `mod_loader_id`: references [`ModLoader`]
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
#[allow(clippy::struct_field_names)]
pub struct Version {
    pub id: Id,
    pub minecraft_version: String,
    pub mod_loader_id: Id,
}

impl DbObject for Version {
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
        "versions"
    }

    fn columns() -> Vec<Column> {
        vec![
            Column::new("id", Type::Id).primary_key(),
            Column::new("minecraft_version", Type::Text).not_null(),
            Column::new("mod_loader_id", Type::Id)
                .not_null()
                .references("mod_loaders(id)"),
        ]
    }
    fn from_row(row: &Row) -> rusqlite::Result<Self>
    where
        Self: Sized,
    {
        Ok(Self {
            id: row.get(0)?,
            minecraft_version: row.get(1)?,
            mod_loader_id: row.get(2)?,
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
            self.minecraft_version
                .to_sql()
                .expect("failed to convert the value to sql"),
            self.mod_loader_id
                .to_sql()
                .expect("failed to convert the value to sql"),
        ]
    }
}

#[allow(clippy::struct_field_names)]
#[derive(Debug, Clone, Deserialize)]
pub struct JsonFrom {
    pub minecraft_version: String,
    pub mod_loader_id: Id,
}

impl FromJson for Version {
    type JsonFrom = JsonFrom;

    fn from_json(data: Self::JsonFrom, _user: User) -> Self {
        Self {
            id: Id::default(),
            minecraft_version: data.minecraft_version,
            mod_loader_id: data.mod_loader_id,
        }
    }
}

impl UpdateJson for Version {
    fn update_with_json(&self, data: Self::JsonFrom) -> Self {
        let mut new = self.clone();
        new.minecraft_version = data.minecraft_version;
        new.mod_loader_id = data.mod_loader_id;
        new
    }
}

impl ApiList for Version {}
impl ApiGet for Version {}
impl ApiCreate for Version {}
impl ApiUpdate for Version {}
impl ApiRemove for Version {}
