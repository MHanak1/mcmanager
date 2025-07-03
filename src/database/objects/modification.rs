use crate::api::handlers::{ApiCreate, ApiGet, ApiIcon, ApiList, ApiObject, ApiRemove, ApiUpdate};
use crate::api::serve::AppState;
use crate::database::objects::{DbObject, FromJson, UpdateJson, User};
use crate::database::types::{Access, Column, Id};
use crate::database::{Database, ValueType};
use axum::Router;
use axum::routing::{get, post};
use serde::{Deserialize, Deserializer, Serialize};
use sqlx::{Arguments, FromRow, IntoArguments};
use std::fmt::Debug;
use std::sync::Arc;

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, FromRow)]
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
            Column::new("id", ValueType::Id).primary_key(),
            Column::new("owner_id", ValueType::Id)
                .not_null()
                .references("users(id)"),
            Column::new("version_id", ValueType::Id)
                .not_null()
                .references("versions(id)"),
            Column::new("name", ValueType::Text).not_null(),
            Column::new("description", ValueType::Text).not_null(),
        ]
    }

    fn id(&self) -> Id {
        self.id
    }
}

impl<'a> IntoArguments<'a, sqlx::Sqlite> for Mod {
    fn into_arguments(self) -> sqlx::sqlite::SqliteArguments<'a> {
        let mut arguments = sqlx::sqlite::SqliteArguments::default();
        arguments.add(self.id).expect("Failed to add argument");
        arguments
            .add(self.owner_id)
            .expect("Failed to add argument");
        arguments
            .add(self.version_id)
            .expect("Failed to add argument");
        arguments.add(self.name).expect("Failed to add argument");
        arguments
            .add(self.description)
            .expect("Failed to add argument");
        arguments
    }
}

impl<'a> IntoArguments<'a, sqlx::Postgres> for Mod {
    fn into_arguments(self) -> sqlx::postgres::PgArguments {
        let mut arguments = sqlx::postgres::PgArguments::default();
        arguments.add(self.id).expect("Failed to add argument");
        arguments
            .add(self.owner_id)
            .expect("Failed to add argument");
        arguments
            .add(self.version_id)
            .expect("Failed to add argument");
        arguments.add(self.name).expect("Failed to add argument");
        arguments
            .add(self.description)
            .expect("Failed to add argument");
        arguments
    }
}

// sqlx::Sqlite value that is present is considered Some value, including null.
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
}

impl FromJson for Mod {
    type JsonFrom = JsonFrom;

    fn from_json(data: &Self::JsonFrom, user: &User) -> Self {
        Self {
            id: Id::default(),
            version_id: data.version_id,
            name: data.name.clone(),
            description: data.description.clone().unwrap_or_default(),
            owner_id: user.id,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct JsonUpdate {
    pub version_id: Option<Id>,
    pub name: Option<String>,
    pub description: Option<String>,
}

impl UpdateJson for Mod {
    type JsonUpdate = JsonUpdate;
    fn update_with_json(&self, data: &Self::JsonUpdate) -> Self {
        let mut new = self.clone();
        new.version_id = data.version_id.unwrap_or(new.version_id);
        new.description = data.description.clone().unwrap_or(new.description);
        new.name = data.name.clone().unwrap_or(new.name);
        new
    }
}

impl ApiObject for Mod {
    fn routes() -> Router<AppState> {
        Router::new()
            .route("/", get(Self::api_list).post(Self::api_create))
            .route(
                "/{id}",
                get(Self::api_get)
                    .put(Self::api_update)
                    .delete(Self::api_remove),
            )
            .route(
                "/{id}/icon",
                post(Self::upload_icon)
                    .put(Self::upload_icon)
                    .get(Self::get_icon),
            )
    }
}

impl ApiList for Mod {}
impl ApiGet for Mod {}
impl ApiCreate for Mod {}
impl ApiUpdate for Mod {}
impl ApiRemove for Mod {}
impl ApiIcon for Mod {}
