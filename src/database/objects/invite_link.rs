use crate::api::handlers::{ApiCreate, ApiGet, ApiList, ApiObject, ApiRemove};
use crate::database::objects::{DbObject, FromJson, User};
use crate::database::types::{Access, Column, Id};
use crate::database::{Database, DatabaseType, ValueType};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{Arguments, FromRow, IntoArguments, Row};
use std::fmt::Debug;
use std::str::FromStr;
use std::sync::Arc;
use uuid::Uuid;
use warp::{Filter, Rejection, Reply};
use warp_rate_limit::RateLimitConfig;

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct InviteLink {
    /// Unique [`Id`] of the invite link
    pub id: Id,
    /// A [`Token`] that allows for creation of an account. expires after use.
    #[serde(with = "uuid::serde::simple")]
    pub invite_token: uuid::Uuid,
    /// The user who created the link
    pub creator_id: Id,
    /// When was the link created (to allow for link expiry)
    pub created: DateTime<Utc>,
}

impl DbObject for InviteLink {
    fn view_access() -> Access {
        Access::Owner("creator_id").or(Access::PrivilegedUser)
    }
    fn update_access() -> Access {
        Access::PrivilegedUser
    }

    fn create_access() -> Access {
        Access::PrivilegedUser
    }

    fn table_name() -> &'static str {
        "invite_links"
    }

    fn columns() -> Vec<Column> {
        vec![
            Column::new("id", ValueType::Id).primary_key(),
            Column::new("invite_token", ValueType::Text)
                .not_null()
                .unique(),
            Column::new("creator_id", ValueType::Id)
                .not_null()
                .references("users(id)"),
            Column::new("created", ValueType::Datetime).not_null(),
            /*
            ("id", "UNSIGNED BIGINT PRIMARY KEY"),
            ("invite_token", "TEXT NOT NULL UNIQUE"),
            ("creator_id", "INTEGER NOT NULL REFERENCES users(id)"),
            ("created", "DATETIME NOT NULL"),
             */
        ]
    }

    fn id(&self) -> Id {
        self.id
    }
}

impl FromRow<'_, <DatabaseType as sqlx::Database>::Row> for InviteLink {
    fn from_row(row: &<DatabaseType as sqlx::Database>::Row) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get(0)?,
            invite_token: row.try_get(1)?,
            creator_id: row.try_get(2)?,
            created: chrono::DateTime::from_str(row.try_get::<&str, _>(3)?).map_err(|err| {
                sqlx::Error::ColumnDecode {
                    index: "created".parse().unwrap(),
                    source: Box::new(err),
                }
            })?,
        })
    }
}

impl<'a> IntoArguments<'a, crate::database::DatabaseType> for InviteLink {
    fn into_arguments(self) -> <crate::database::DatabaseType as sqlx::Database>::Arguments<'a> {
        let mut arguments = <crate::database::DatabaseType as sqlx::Database>::Arguments::default();
        arguments.add(self.id).expect("Failed to add argument");
        arguments
            .add(self.invite_token)
            .expect("Failed to add argument");
        arguments
            .add(self.creator_id)
            .expect("Failed to add argument");
        arguments
            .add(self.created.to_string())
            .expect("Failed to add argument");
        arguments
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct JsonFrom {}

impl FromJson for InviteLink {
    type JsonFrom = JsonFrom;

    fn from_json(_data: &Self::JsonFrom, user: &User) -> Self {
        Self {
            id: Id::default(),
            invite_token: Uuid::new_v4(),
            creator_id: user.id,
            created: chrono::offset::Utc::now(),
        }
    }
}

impl ApiObject for InviteLink {
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
            .or(Self::remove_filter(database, rate_limit_config.clone()))
    }
}
impl ApiList for InviteLink {}
impl ApiGet for InviteLink {}
impl ApiCreate for InviteLink {}
impl ApiRemove for InviteLink {}
