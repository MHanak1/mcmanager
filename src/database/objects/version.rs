use crate::api::handlers::{ApiCreate, ApiGet, ApiList, ApiObject, ApiRemove, ApiUpdate};
use crate::database::objects::{DbObject, FromJson, UpdateJson, User};
use crate::database::types::{Access, Column, Id};
use crate::database::{Database, ValueType};
use serde::{Deserialize, Serialize};
use sqlx::{Arguments, FromRow, IntoArguments};
use std::sync::Arc;
use warp::{Filter, Rejection, Reply};
use warp_rate_limit::RateLimitConfig;

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

    fn columns() -> Vec<Column> {
        vec![
            Column::new("id", ValueType::Id).primary_key(),
            Column::new("minecraft_version", ValueType::Text).not_null(),
            Column::new("mod_loader_id", ValueType::Id)
                .not_null()
                .references("mod_loaders(id)"),
        ]
    }
    fn id(&self) -> Id {
        self.id
    }
}

impl<'a> IntoArguments<'a, crate::database::DatabaseType> for Version {
    fn into_arguments(self) -> <crate::database::DatabaseType as sqlx::Database>::Arguments<'a> {
        let mut arguments = <crate::database::DatabaseType as sqlx::Database>::Arguments::default();
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
    fn filters(
        database: Arc<Database>,
        rate_limit_config: RateLimitConfig,
    ) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone {
        Self::list_filter(database.clone(), rate_limit_config.clone())
            .or(Self::get_filter(
                database.clone(),
                rate_limit_config.clone(),
            ))
            .or(Self::create_filter(
                database.clone(),
                rate_limit_config.clone(),
            ))
            .or(Self::update_filter(
                database.clone(),
                rate_limit_config.clone(),
            ))
            .or(Self::remove_filter(database, rate_limit_config.clone()))
    }
}

impl ApiList for Version {}
impl ApiGet for Version {}
impl ApiCreate for Version {}
impl ApiUpdate for Version {}
impl ApiRemove for Version {}
