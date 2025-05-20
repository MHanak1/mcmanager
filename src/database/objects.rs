use crate::database::Database;
use crate::database::types::{Access, Column, Id};
use rusqlite::Row;
use rusqlite::types::ToSqlOutput;
use serde::de::DeserializeOwned;

pub mod invite_link;
pub mod mod_loader;
pub mod modification;
pub mod user;
pub mod version;
pub mod world;

pub use self::{
    invite_link::InviteLink, mod_loader::ModLoader, modification::Mod, user::Password,
    user::Session, user::User, version::Version, world::World,
};

/// An object that is meant to be stored in a database
/// the object must have a unique Id, by default in the first column
#[allow(dead_code)]
pub trait DbObject {
    /// [`Access`] level dictating which users can create the object using th api.
    fn view_access() -> Access;
    /// [`Access`] level for updating and removing of the object.
    fn update_access() -> Access;
    /// [`Access`] level for creating of the object.
    fn create_access() -> Access;
    /// whether a user can view this object using the API
    ///
    /// # Panics
    ///
    /// see [`Access::can_access`]
    fn can_view(&self, user: &User) -> bool {
        Self::view_access().can_access::<Self>(Some(self), user)
    }
    /// whether a user can update this object using the API
    ///
    /// # Panics
    ///
    /// see [`Access::can_access`]
    fn can_update(&self, user: &User) -> bool {
        Self::update_access().can_access::<Self>(Some(self), user)
    }
    /// whether a user can create this object using the API
    ///
    /// # Panics
    ///
    /// see [`Access::can_access`]
    fn can_create(user: &User) -> bool {
        Self::create_access().can_access::<Self>(None, user)
    }

    #[allow(unused)]
    fn before_create(&self, database: &Database) {}
    #[allow(unused)]
    fn before_update(&self, database: &Database) {}
    #[allow(unused)]
    fn before_delete(&self, database: &Database) {}
    #[allow(unused)]
    fn after_create(&self, database: &Database) {}
    #[allow(unused)]
    fn after_update(&self, database: &Database) {}
    #[allow(unused)]
    fn after_delete(&self, database: &Database) {}

    fn table_name() -> &'static str;

    fn columns() -> Vec<Column>;
    /// Returns object's Id
    fn from_row(row: &Row) -> rusqlite::Result<Self>
    where
        Self: Sized;
    fn get_id(&self) -> Id;
    fn id_column_index() -> usize {
        0
    }
    fn get_column(name: &str) -> Option<Column> {
        Self::columns()
            .iter()
            .find(|c| c.name == name)
            .map(|c| c.to_owned())
    }

    fn get_column_index(name: &str) -> Option<usize> {
        Self::columns().iter().position(|c| c.name == name)
    }

    fn database_descriptor() -> String {
        Self::columns()
            .iter()
            .map(|column| format!("{} {}", column.name, column.descriptor()))
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn params(&self) -> Vec<ToSqlOutput>;
}

pub trait FromJson
where
    Self: Sized,
{
    type JsonFrom: Clone + DeserializeOwned + Send;

    fn from_json(data: &Self::JsonFrom, user: &User) -> Self;
}

pub trait UpdateJson
where
    Self: Sized,
{
    type JsonUpdate: Clone + DeserializeOwned + Send;
    fn update_with_json(&self, data: &Self::JsonUpdate) -> Self;
}
