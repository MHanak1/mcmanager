use crate::api::handlers::json_fields;
use crate::database::types::{Access, Column, Id, Token, Type};
use chrono::{DateTime, Utc};
use rusqlite::types::ToSqlOutput;
use rusqlite::{Row, ToSql};
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use crate::database::objects::{DbObject, FromJson, User};

/// `id`: unique [`Id`] of the invite link
///
/// `invite_token`: a [`Token`] that allows for creation of an account. expires after use.
///
/// `creator_id`: the user who created the link
///
/// `created`: when was the link created (to allow for link expiry)
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct InviteLink {
    pub id: Id,
    pub invite_token: Token,
    pub creator_id: Id,
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
            Column::new("id", Type::Id).primary_key(),
            Column::new("invite_token", Type::Text).not_null().unique(),
            Column::new("creator_id", Type::Id)
                .not_null()
                .references("users(id)"),
            Column::new("created", Type::Datetime).not_null(),
            /*
            ("id", "UNSIGNED BIGINT PRIMARY KEY"),
            ("invite_token", "TEXT NOT NULL UNIQUE"),
            ("creator_id", "INTEGER NOT NULL REFERENCES users(id)"),
            ("created", "DATETIME NOT NULL"),
             */
        ]
    }

    fn from_row(row: &Row) -> rusqlite::Result<Self>
    where
        Self: Sized,
    {
        Ok(Self {
            id: row.get(0)?,
            invite_token: row.get(1)?,
            creator_id: row.get(2)?,
            created: row.get(3)?,
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
            self.invite_token
                .to_sql()
                .expect("failed to convert the value to sql"),
            self.creator_id
                .to_sql()
                .expect("failed to convert the value to sql"),
            self.created
                .to_sql()
                .expect("failed to convert the value to sql"),
        ]
    }
}

impl FromJson for InviteLink {
    type JsonFrom = json_fields::InviteLink;

    fn from_json(_data: Self::JsonFrom, user: User) -> Self {
        Self {
            id: Id::default(),
            invite_token: Token::default(),
            creator_id: user.id,
            created: chrono::offset::Utc::now(),
        }
    }
}