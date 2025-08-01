use crate::api::handlers::{ApiCreate, ApiGet, ApiList, ApiObject, ApiRemove, ApiUpdate};
use crate::api::serve::AppState;
use crate::database::objects::{DbObject, FromJson, UpdateJson, User};
use crate::database::types::{Access, Column, Id};
use crate::database::{Cachable, ValueType};
use axum::Router;
use axum::routing::get;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use sqlx::{Arguments, FromRow, IntoArguments};
use std::any::Any;
use std::fmt::Debug;

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, FromRow)]
pub struct ModLoader {
    /// The mod loader's unique [`Id`]
    pub id: Id,
    /// Mod loader's name (like "Fabric" or "Vanilla")
    pub name: String,
    /// If the mod loader actually can load mods (Generally false for Vanilla)
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

    const COLUMNS: Lazy<Vec<Column>> = Lazy::new(|| {
        vec![
            Column::new("id", ValueType::Id).primary_key(),
            Column::new("name", ValueType::Text).not_null(),
            Column::new("can_load_mods", ValueType::Boolean)
                .not_null()
                .default("false"),
        ]
    });
    fn id(&self) -> Id {
        self.id
    }
}

impl Cachable for ModLoader {
    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self as Box<dyn Any>
    }
}

impl<'a> IntoArguments<'a, sqlx::Sqlite> for ModLoader {
    fn into_arguments(self) -> sqlx::sqlite::SqliteArguments<'a> {
        let mut arguments = sqlx::sqlite::SqliteArguments::default();
        arguments.add(self.id).expect("Failed to add argument");
        arguments.add(self.name).expect("Failed to add argument");
        arguments
            .add(self.can_load_mods)
            .expect("Failed to add argument");
        arguments
    }
}

impl<'a> IntoArguments<'a, sqlx::Postgres> for ModLoader {
    fn into_arguments(self) -> sqlx::postgres::PgArguments {
        let mut arguments = sqlx::postgres::PgArguments::default();
        arguments.add(self.id).expect("Failed to add argument");
        arguments.add(self.name).expect("Failed to add argument");
        arguments
            .add(self.can_load_mods)
            .expect("Failed to add argument");
        arguments
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

impl ApiObject for ModLoader {
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

impl ApiList for ModLoader {}
impl ApiGet for ModLoader {}
impl ApiCreate for ModLoader {}
impl ApiUpdate for ModLoader {}
impl ApiRemove for ModLoader {}
