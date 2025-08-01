use crate::api::handlers::{ApiCreate, ApiGet, ApiIcon, ApiList, ApiObject, ApiRemove, ApiUpdate};
use crate::api::serve::AppState;
use crate::database::objects::{DbObject, FromJson, ModLoader, UpdateJson, User};
use crate::database::types::{Access, Column, Id};
use crate::database::{Cachable, Database, DatabaseError, ValueType};
use async_trait::async_trait;
use axum::Router;
use axum::http::StatusCode;
use axum::routing::{get, post};
use once_cell::sync::Lazy;
use serde::{Deserialize, Deserializer, Serialize};
use sqlx::{Arguments, FromRow, IntoArguments};
use std::any::Any;
use std::fmt::Debug;
use std::sync::Arc;
use axum::extract::DefaultBodyLimit;

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
    /// Modrinth project's id or slug
    pub modrinth_id: Option<String>,
    /// Whether the mod is accessible to all user, or just the owner
    pub public: bool,
}

impl DbObject for Mod {
    fn view_access() -> Access {
        Access::Owner("owner_id")
            .or(Access::IfPublic("public"))
            .or(Access::PrivilegedUser)
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

    const COLUMNS: Lazy<Vec<Column>> = Lazy::new(|| {
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
            Column::new("modrinth_id", ValueType::Text),
            Column::new("public", ValueType::Boolean).not_null(),
        ]
    });

    fn id(&self) -> Id {
        self.id
    }

    fn owner_id(&self) -> Option<Id> {
        Some(self.owner_id)
    }
    fn is_public(&self) -> bool {
        self.public
    }
}

impl Cachable for Mod {
    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self as Box<dyn Any>
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
            .add(self.modrinth_id)
            .expect("Failed to add argument");
        arguments.add(self.public).expect("Failed to add argument");
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
            .add(self.modrinth_id)
            .expect("Failed to add argument");
        arguments.add(self.public).expect("Failed to add argument");
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
    pub modrinth_id: Option<String>,
    pub public: Option<bool>,
}

impl FromJson for Mod {
    type JsonFrom = JsonFrom;

    fn from_json(data: &Self::JsonFrom, user: &User) -> Self {
        Self {
            id: Id::default(),
            version_id: data.version_id,
            name: data.name.clone(),
            description: data.description.clone().unwrap_or_default(),
            modrinth_id: data.modrinth_id.clone(),
            public: data.public.unwrap_or(false),
            owner_id: user.id,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct JsonUpdate {
    pub version_id: Option<Id>,
    pub name: Option<String>,
    pub description: Option<String>,
    #[serde(deserialize_with = "deserialize_some")]
    pub modrinth_id: Option<Option<String>>,
    pub public: Option<bool>,
}

impl UpdateJson for Mod {
    type JsonUpdate = JsonUpdate;
    fn update_with_json(&self, data: &Self::JsonUpdate) -> Self {
        let mut new = self.clone();
        new.version_id = data.version_id.unwrap_or(new.version_id);
        new.description = data.description.clone().unwrap_or(new.description);
        new.name = data.name.clone().unwrap_or(new.name);
        new.modrinth_id = data.modrinth_id.clone().unwrap_or(new.modrinth_id);
        new.public = data.public.unwrap_or(new.public);
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
                    .patch(Self::api_update)
                    .delete(Self::api_remove),
            )
            .route(
                "/{id}/icon",
                post(Self::upload_icon)
                    .patch(Self::upload_icon)
                    .get(Self::get_icon),
            ).layer(DefaultBodyLimit::max(8*1024*1024))
            .route(
                "/default/icon",
                get(Self::default_icon)
            )
    }
}

impl ApiList for Mod {}
impl ApiGet for Mod {}
#[async_trait]
impl ApiCreate for Mod {
    async fn before_api_create(
        state: AppState,
        json: &mut Self::JsonFrom,
        user: &User,
    ) -> Result<(), DatabaseError> {
        let group = user.group(state.database, None).await;

        if !group.can_upload_mods {
            return Err(DatabaseError::Unauthorized);
        }

        if !group.is_privileged {
            json.public = Some(false);
        }

        Ok(())
    }
}
#[async_trait]
impl ApiUpdate for Mod {
    async fn before_api_update(
        &self,
        state: AppState,
        json: &mut Self::JsonUpdate,
        user: &User,
    ) -> Result<(), DatabaseError> {
        let group = user.group(state.database, None).await;
        if !group.is_privileged {
            json.public = Some(false);
        }

        Ok(())
    }
}
impl ApiRemove for Mod {}
impl ApiIcon for Mod {
    const DEFAULT_ICON_BYTES: &'static [u8] = include_bytes!("../../resources/icons/mod_default.png");
    const DEFAULT_ICON_MIME: &'static str = "image/png";
}
