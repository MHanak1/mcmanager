use crate::api::handlers::{ApiCreate, ApiGet, ApiList, ApiObject, ApiRemove, ApiUpdate};
use crate::api::serve::AppState;
use crate::database::objects::{DbObject, FromJson, ModLoader, UpdateJson, User};
use crate::database::types::{Access, Column, Id};
use crate::database::{Cachable, ValueType};
use axum::Router;
use axum::routing::get;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use sqlx::{Arguments, FromRow, IntoArguments};
use std::any::Any;

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, FromRow)]
#[allow(clippy::struct_field_names)]
pub struct Version {
    /// version's unique [`Id`]
    pub id: Id,
    /// version string displayed to the client (like "1.20.1")
    pub minecraft_version: String,
    /// which [`ModLoader`] does the version use
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

    const COLUMNS: Lazy<Vec<Column>> = Lazy::new(|| {
        vec![
            Column::new("id", ValueType::Id).primary_key(),
            Column::new("minecraft_version", ValueType::Text).not_null(),
            Column::new("mod_loader_id", ValueType::Id)
                .not_null()
                .references("mod_loaders(id)"),
        ]
    });

    fn id(&self) -> Id {
        self.id
    }
}

impl Cachable for Version {
    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self as Box<dyn Any>
    }
}

impl<'a> IntoArguments<'a, sqlx::Sqlite> for Version {
    fn into_arguments(self) -> sqlx::sqlite::SqliteArguments<'a> {
        let mut arguments = sqlx::sqlite::SqliteArguments::default();
        arguments.add(self.id).expect("Failed to add argument");
        arguments
            .add(self.minecraft_version)
            .expect("Failed to argument");
        arguments
            .add(self.mod_loader_id)
            .expect("Failed to argument");
        arguments
    }
}

impl<'a> IntoArguments<'a, sqlx::Postgres> for Version {
    fn into_arguments(self) -> sqlx::postgres::PgArguments {
        let mut arguments = sqlx::postgres::PgArguments::default();
        arguments.add(self.id).expect("Failed to add argument");
        arguments
            .add(self.minecraft_version)
            .expect("Failed to argument");
        arguments
            .add(self.mod_loader_id)
            .expect("Failed to argument");
        arguments
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

    fn from_json(data: &Self::JsonFrom, _user: &User) -> Self {
        Self {
            id: Id::default(),
            minecraft_version: data.minecraft_version.clone(),
            mod_loader_id: data.mod_loader_id,
        }
    }
}
#[allow(clippy::struct_field_names)]
#[derive(Debug, Clone, Deserialize)]
pub struct JsonUpdate {
    pub minecraft_version: Option<String>,
    pub mod_loader_id: Option<Id>,
}

impl UpdateJson for Version {
    type JsonUpdate = JsonUpdate;
    fn update_with_json(&self, data: &Self::JsonUpdate) -> Self {
        let mut new = self.clone();
        new.minecraft_version = data
            .minecraft_version
            .clone()
            .unwrap_or(new.minecraft_version);
        new.mod_loader_id = data.mod_loader_id.unwrap_or(new.mod_loader_id);
        new
    }
}

impl ApiObject for Version {
    fn routes() -> Router<AppState> {
        Router::new()
            .route("/", get(Self::api_list).post(Self::api_create))
            .route(
                "/{id}",
                get(Self::api_get)
                    .patch(Self::api_update)
                    .delete(Self::api_remove),
            )
    }
}

impl ApiList for Version {}
impl ApiGet for Version {}
impl ApiCreate for Version {}
impl ApiUpdate for Version {}
impl ApiRemove for Version {}
