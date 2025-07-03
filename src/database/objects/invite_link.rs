use crate::api::handlers::{ApiCreate, ApiGet, ApiList, ApiObject, ApiRemove};
use crate::api::serve::AppState;
use crate::database::objects::{DbObject, FromJson, User};
use crate::database::types::{Access, Column, Id};
use crate::database::{Database, ValueType};
use axum::Router;
use axum::routing::get;
use chrono::{DateTime, Utc};
use duplicate::duplicate_item;
use serde::{Deserialize, Serialize};
use sqlx::{Arguments, FromRow, IntoArguments, Row};
use std::fmt::Debug;
use std::str::FromStr;
use std::sync::Arc;
use uuid::Uuid;

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
            Column::new("invite_token", ValueType::Token)
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

#[duplicate_item(Row; [sqlx::sqlite::SqliteRow]; [sqlx::postgres::PgRow])]
impl FromRow<'_, Row> for InviteLink {
    fn from_row(row: &Row) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get(0)?,
            invite_token: row.try_get(1)?,
            creator_id: row.try_get(2)?,
            created: row.try_get(3)?,
        })
    }
}

impl<'a> IntoArguments<'a, sqlx::Sqlite> for InviteLink {
    fn into_arguments(self) -> sqlx::sqlite::SqliteArguments<'a> {
        let mut arguments = sqlx::sqlite::SqliteArguments::default();
        arguments.add(self.id).expect("Failed to add argument");
        arguments
            .add(self.invite_token)
            .expect("Failed to add argument");
        arguments
            .add(self.creator_id)
            .expect("Failed to add argument");
        arguments.add(self.created).expect("Failed to add argument");
        arguments
    }
}

impl<'a> IntoArguments<'a, sqlx::Postgres> for InviteLink {
    fn into_arguments(self) -> sqlx::postgres::PgArguments {
        let mut arguments = sqlx::postgres::PgArguments::default();
        arguments.add(self.id).expect("Failed to add argument");
        arguments
            .add(self.invite_token)
            .expect("Failed to add argument");
        arguments
            .add(self.creator_id)
            .expect("Failed to add argument");
        arguments.add(self.created).expect("Failed to add argument");
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
    fn routes() -> Router<AppState> {
        Router::new()
            .route("/", get(Self::api_list).post(Self::api_create))
            .route("/{id}", get(Self::api_get).delete(Self::api_remove))
    }
}
impl ApiList for InviteLink {}
impl ApiGet for InviteLink {}
impl ApiCreate for InviteLink {}
impl ApiRemove for InviteLink {}
