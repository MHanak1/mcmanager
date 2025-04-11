use crate::database::types::{Access, Column, Id, Token, Type};
use argon2::password_hash::SaltString;
use chrono::{DateTime, Utc};
use rusqlite::types::ToSqlOutput;
use rusqlite::{Connection, Row, ToSql, params, params_from_iter};
use serde::{Deserialize, Serialize};
use std::fmt::Debug;

/// An object that is ment to be stored in a database
/// the object must have a unique Id, by default in the first column
#[allow(dead_code)]
pub trait DbObject {
    fn get_id(&self) -> Id;

    fn can_create(user: &User) -> bool {
        Self::create_access().can_access::<Self>(None, user)
    }

    fn view_access() -> Access;
    fn update_access() -> Access;
    fn create_access() -> Access;

    fn from_row(row: &Row) -> rusqlite::Result<Self>
    where
        Self: Sized;
    fn table_name() -> &'static str;

    fn columns() -> Vec<Column>;
    fn id_column_index() -> usize {
        0
    }
    fn owner_id_column_index() -> Option<usize> {
        None
    }
    fn get_column(name: &str) -> Option<Column> {
        for column in Self::columns() {
            if name == column.name {
                return Some(column);
            }
        }
        None
    }

    fn database_descriptor() -> String {
        Self::columns()
            .iter()
            .map(|column| format!("{} {}", column.name, column.descriptor()))
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn params(&self) -> Vec<ToSqlOutput>;
    fn remove_self(&self, conn: &Connection, user: Option<&User>) -> rusqlite::Result<usize> {
        conn.execute(
            &format!(
                "DELETE FROM {} WHERE {} = ?1{}",
                Self::table_name(),
                Self::columns()[Self::id_column_index()].name,
                match user {
                    Some(user) => {
                        format!(" AND {}", Self::update_access().access_filter::<Self>(user))
                    }
                    None => "".to_string(),
                },
            ),
            params![self.get_id()],
        )
    }
    fn insert_self(&self, conn: &Connection, user: Option<&User>) -> rusqlite::Result<usize> {
        if let Some(user) = user {
            if Self::can_create(user) {
                return Err(rusqlite::Error::InvalidQuery);
            }
        }
        conn.execute(
            &format!(
                "INSERT INTO {} ({}) VALUES ({})",
                Self::table_name(),
                Self::columns()
                    .iter()
                    .map(|column| column.name.to_string())
                    .collect::<Vec<String>>()
                    .join(", "),
                Self::columns()
                    .iter()
                    .enumerate()
                    .map(|i| { format!("?{}", i.0 + 1) })
                    .collect::<Vec<String>>()
                    .join(", ")
            ),
            params_from_iter(self.params()),
        )
    }

    fn update_self(&self, conn: &Connection, user: Option<&User>) -> rusqlite::Result<usize> {
        conn.execute(
            &format!(
                "UPDATE {} SET {} WHERE {} = {}{}",
                Self::table_name(),
                Self::columns()
                    .iter()
                    .enumerate()
                    .map(|(id, column)| { format!("{} = ?{}", column.name, id + 1) })
                    .collect::<Vec<String>>()
                    .join(", "),
                Self::columns()[Self::id_column_index()].name,
                self.get_id().as_i64(),
                match user {
                    Some(user) => {
                        format!(" AND {}", Self::update_access().access_filter::<Self>(user))
                    }
                    None => "".to_string(),
                }
            ),
            params_from_iter(self.params()),
        )
    }
    fn get_from_db(conn: &Connection, id: Id, user: Option<&User>) -> rusqlite::Result<Self>
    where
        Self: Sized,
    {
        conn.query_row(
            &format!(
                "SELECT * FROM {} WHERE {} = ?1{}",
                Self::table_name(),
                Self::columns()[Self::id_column_index()].name,
                match user {
                    Some(user) => {
                        format!(" AND {}", Self::update_access().access_filter::<Self>(user))
                    }
                    None => "".to_string(),
                }
            ),
            params![id],
            |row| Self::from_row(row),
        )
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct Mod {
    pub id: Id,
    pub version_id: Id,
    pub name: String,
    pub description: String,
    pub icon_id: Option<Id>,
    pub owner_id: Id,
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
#[allow(clippy::struct_field_names)]
pub struct Version {
    pub id: Id,
    pub minecraft_version: String,
    pub mod_loader_id: Id,
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct ModLoader {
    pub id: Id,
    pub name: String,
    pub can_load_mods: bool,
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct World {
    pub id: Id,
    pub owner_id: Id,
    pub name: String,
    pub icon_id: Option<Id>,
    pub allocated_memory: u32, //Memory in MiB
    pub version_id: Id,
    pub enabled: bool,
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: Id,
    pub name: String,
    pub avatar_id: Option<Id>,
    pub memory_limit: Option<u32>,
    pub player_limit: Option<u32>,
    pub world_limit: Option<u32>,
    pub active_world_limit: Option<u32>,
    pub storage_limit: Option<u32>,
    pub is_privileged: bool,
    pub enabled: bool,
}

//password is in a separate object to avoid accidentally sending those values together with user data
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Password {
    pub user_id: Id,
    pub salt: SaltString,
    pub hash: String,
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct Session {
    pub user_id: Id,
    pub token: Token,
    pub created: DateTime<Utc>,
    pub expires: bool,
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct InviteLink {
    pub id: Id,
    pub invite_token: Token,
    pub creator_id: Id,
    pub created: DateTime<Utc>,
}

impl DbObject for Mod {
    fn get_id(&self) -> Id {
        self.id
    }

    fn can_create(user: &User) -> bool {
        user.enabled
    }

    fn view_access() -> Access {
        Access::Owner.or(Access::PrivilegedUser)
    }

    fn update_access() -> Access {
        Access::Owner.or(Access::PrivilegedUser)
    }

    fn create_access() -> Access {
        Access::User
    }

    fn from_row(row: &Row) -> rusqlite::Result<Self>
    where
        Self: Sized,
    {
        Ok(Self {
            id: row.get(0)?,
            owner_id: row.get(1)?,
            version_id: row.get(2)?,
            name: row.get(3)?,
            description: row.get(4)?,
            icon_id: row.get(5)?,
        })
    }

    fn table_name() -> &'static str {
        "mods"
    }
    fn columns() -> Vec<Column> {
        vec![
            Column::new("id", Type::Id).primary_key(),
            Column::new("owner_id", Type::Id)
                .not_null()
                .references("users(id)"),
            Column::new("version_id", Type::Id)
                .not_null()
                .references("versions(id)"),
            Column::new("name", Type::Text).not_null(),
            Column::new("description", Type::Text).not_null(),
            Column::new("icon_id", Type::Id),
        ]
    }
    fn owner_id_column_index() -> Option<usize> {
        Some(1)
    }
    fn params(&self) -> Vec<ToSqlOutput> {
        vec![
            self.id.to_sql().unwrap(),
            self.owner_id.to_sql().unwrap(),
            self.version_id.to_sql().unwrap(),
            self.name.to_sql().unwrap(),
            self.description.to_sql().unwrap(),
            self.icon_id.to_sql().unwrap(),
        ]
    }
}

impl DbObject for Version {
    fn get_id(&self) -> Id {
        self.id
    }
    fn view_access() -> Access {
        Access::User
    }

    fn update_access() -> Access {
        Access::PrivilegedUser
    }

    fn create_access() -> Access {
        Access::PrivilegedUser
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

    fn params(&self) -> Vec<ToSqlOutput> {
        vec![
            self.id.to_sql().unwrap(),
            self.minecraft_version.to_sql().unwrap(),
            self.mod_loader_id.to_sql().unwrap(),
        ]
    }
}

impl DbObject for ModLoader {
    fn get_id(&self) -> Id {
        self.id
    }

    fn view_access() -> Access {
        Access::User
    }

    fn update_access() -> Access {
        Access::PrivilegedUser
    }

    fn create_access() -> Access {
        Access::PrivilegedUser
    }

    fn from_row(row: &Row) -> rusqlite::Result<Self>
    where
        Self: Sized,
    {
        Ok(Self {
            id: row.get(0)?,
            name: row.get(1)?,
            can_load_mods: row.get(2)?,
        })
    }
    fn table_name() -> &'static str {
        "mod_loaders"
    }

    fn columns() -> Vec<Column> {
        vec![
            Column::new("id", Type::Id).primary_key(),
            Column::new("name", Type::Text).not_null(),
            Column::new("can_load_mods", Type::Boolean)
                .not_null()
                .default("false"),
        ]
    }

    fn params(&self) -> Vec<ToSqlOutput> {
        vec![
            self.id.to_sql().unwrap(),
            self.name.to_sql().unwrap(),
            self.can_load_mods.to_sql().unwrap(),
        ]
    }
}

impl DbObject for World {
    fn get_id(&self) -> Id {
        self.id
    }

    fn view_access() -> Access {
        Access::Owner.or(Access::PrivilegedUser)
    }

    fn update_access() -> Access {
        Access::Owner.or(Access::PrivilegedUser)
    }

    fn create_access() -> Access {
        Access::User
    }

    fn from_row(row: &Row) -> rusqlite::Result<Self>
    where
        Self: Sized,
    {
        Ok(Self {
            id: row.get(0)?,
            owner_id: row.get(1)?,
            name: row.get(2)?,
            icon_id: row.get(3)?,
            allocated_memory: row.get(4)?,
            version_id: row.get(5)?,
            enabled: row.get(6)?,
        })
    }

    fn table_name() -> &'static str {
        "worlds"
    }
    fn columns() -> Vec<Column> {
        vec![
            Column::new("id", Type::Id).primary_key(),
            Column::new("owner_id", Type::Id)
                .not_null()
                .references("users(id)"),
            Column::new("name", Type::Text).not_null(),
            Column::new("icon_id", Type::Id),
            Column::new("allocated_memory", Type::Integer(false)),
            Column::new("version_id", Type::Id)
                .not_null()
                .references("versions(id)"),
            Column::new("enabled", Type::Boolean)
                .not_null()
                .default("false"),
            /*
            ("id", "UNSIGNED BIGINT PRIMARY KEY"),
            ("owner_id", "UNSIGNED BIGINT NOT NULL REFERENCES users(id)"),
            ("name", "TEXT NOT NULL"),
            ("icon_id", "UNSIGNED BIGINT"),
            ("allocated_memory", "UNSIGNED INTEGER"),
            (
                "version_id",
                "UNSIGNED BIGINT NOT NULL REFERENCES versions(id)",
            ),
            ("enabled", "BOOLEAN NOT NULL DEFAULT FALSE"),
             */
        ]
    }
    fn owner_id_column_index() -> Option<usize> {
        Some(1)
    }
    fn params(&self) -> Vec<ToSqlOutput> {
        vec![
            self.id.to_sql().unwrap(),
            self.owner_id.to_sql().unwrap(),
            self.name.to_sql().unwrap(),
            self.icon_id.to_sql().unwrap(),
            self.allocated_memory.to_sql().unwrap(),
            self.version_id.to_sql().unwrap(),
            self.enabled.to_sql().unwrap(),
        ]
    }
}
impl DbObject for User {
    fn get_id(&self) -> Id {
        self.id
    }

    fn view_access() -> Access {
        Access::Owner.or(Access::PrivilegedUser)
    }

    fn update_access() -> Access {
        Access::Owner.or(Access::PrivilegedUser)
    }

    fn create_access() -> Access {
        Access::PrivilegedUser
    }

    fn from_row(row: &Row) -> rusqlite::Result<Self>
    where
        Self: Sized,
    {
        Ok(Self {
            id: row.get(0)?,
            name: row.get(1)?,
            avatar_id: row.get(2)?,
            memory_limit: row.get(3)?,
            player_limit: row.get(4)?,
            world_limit: row.get(5)?,
            active_world_limit: row.get(6)?,
            storage_limit: row.get(7)?,
            is_privileged: row.get(8)?,
            enabled: row.get(9)?,
        })
    }
    fn table_name() -> &'static str {
        "users"
    }

    fn columns() -> Vec<Column> {
        vec![
            Column::new("id", Type::Id).primary_key(),
            Column::new("name", Type::Text).not_null().unique(),
            Column::new("avatar_id", Type::Id),
            Column::new("memory_limit", Type::Integer(false)),
            Column::new("player_limit", Type::Integer(false)),
            Column::new("world_limit", Type::Integer(false)),
            Column::new("active_world_limit", Type::Integer(false)),
            Column::new("storage_limit", Type::Integer(false)),
            Column::new("is_privileged", Type::Boolean)
                .not_null()
                .default("false"),
            Column::new("enabled", Type::Boolean)
                .not_null()
                .default("true"),
            /*
            ("id", "UNSIGNED BIGINT PRIMARY KEY"),
            ("name", "TEXT NOT NULL UNIQUE"),
            ("avatar_id", "UNSIGNED BIGINT"),
            ("memory_limit", "UNSIGNED INTEGER"),
            ("player_limit", "UNSIGNED INTEGER"),
            ("world_limit", "UNSIGNED INTEGER"),
            ("active_world_limit", "UNSIGNED INTEGER"),
            ("storage_limit", "UNSIGNED INTEGER"),
            ("is_privileged", "BOOLEAN NOT NULL DEFAULT FALSE"),
            ("enabled", "BOOLEAN NOT NULL DEFAULT TRUE"),
             */
        ]
    }
    //in this case the user's owner is the user themselves
    fn owner_id_column_index() -> Option<usize> {
        Some(0)
    }

    fn params(&self) -> Vec<ToSqlOutput> {
        vec![
            self.id.to_sql().unwrap(),
            self.name.to_sql().unwrap(),
            self.avatar_id.to_sql().unwrap(),
            self.memory_limit.to_sql().unwrap(),
            self.player_limit.to_sql().unwrap(),
            self.world_limit.to_sql().unwrap(),
            self.active_world_limit.to_sql().unwrap(),
            self.storage_limit.to_sql().unwrap(),
            self.is_privileged.to_sql().unwrap(),
            self.enabled.to_sql().unwrap(),
        ]
    }
}

impl Default for User {
    fn default() -> Self {
        Self {
            id: Default::default(),
            name: String::new(),
            avatar_id: None,
            memory_limit: None,
            player_limit: None,
            world_limit: None,
            active_world_limit: None,
            storage_limit: None,
            is_privileged: false,
            enabled: true,
        }
    }
}

impl DbObject for Password {
    fn get_id(&self) -> Id {
        self.user_id
    }

    fn view_access() -> Access {
        Access::None
    }

    fn update_access() -> Access {
        Access::None
    }

    fn create_access() -> Access {
        Access::None
    }

    fn from_row(row: &Row) -> rusqlite::Result<Self>
    where
        Self: Sized,
    {
        Ok(Self {
            user_id: row.get(0)?,
            salt: match SaltString::from_b64(row.get::<usize, String>(1)?.as_str()) {
                Ok(result) => result,
                Err(_) => {
                    return Err(rusqlite::Error::InvalidQuery);
                }
            },
            hash: row.get(2)?,
        })
    }

    fn table_name() -> &'static str {
        "passwords"
    }

    fn columns() -> Vec<Column> {
        vec![
            Column::new("user_id", Type::Id)
                .primary_key()
                .references("users(id)"),
            Column::new("salt", Type::Text).not_null(),
            Column::new("hash", Type::Text).not_null(),
            /*
            (
                "user_id",
                "UNSIGNED BIGINT PRIMARY KEY REFERENCES users(id)",
            ),
            ("salt", "TEXT NOT NULL"),
            ("hash", "TEXT NOT NULL"),
             */
        ]
    }
    fn owner_id_column_index() -> Option<usize> {
        Some(0)
    }

    fn params(&self) -> Vec<ToSqlOutput> {
        vec![
            self.user_id.to_sql().unwrap(),
            self.salt.as_str().to_sql().unwrap(),
            self.hash.to_sql().unwrap(),
        ]
    }
}

impl DbObject for Session {
    fn get_id(&self) -> Id {
        self.user_id
    }

    fn view_access() -> Access {
        Access::Owner.or(Access::PrivilegedUser)
    }

    fn update_access() -> Access {
        Access::PrivilegedUser
    }

    //TODO: this probably should be creatable by users as well
    fn create_access() -> Access {
        Access::PrivilegedUser
    }

    fn from_row(row: &Row) -> rusqlite::Result<Self>
    where
        Self: Sized,
    {
        Ok(Self {
            user_id: row.get(0)?,
            token: row.get(1)?,
            created: row.get(2)?,
            expires: row.get(3)?,
        })
    }

    fn table_name() -> &'static str {
        "sessions"
    }

    fn columns() -> Vec<Column> {
        vec![
            Column::new("user_id", Type::Id).references("users(id)"),
            Column::new("token", Type::Text).primary_key(),
            Column::new("created", Type::Datetime).not_null(),
            Column::new("expires", Type::Boolean)
                .not_null()
                .default("true"),
            /*
            ("user_id", "UNSIGNED INTEGER REFERENCES users(id)"),
            ("token", "TEXT  PRIMARY KEY"),
            ("created", "DATETIME NOT NULL"),
            ("expires", "BOOLEAN NOT NULL DEFAULT TRUE"),
             */
        ]
    }
    fn owner_id_column_index() -> Option<usize> {
        Some(0)
    }

    fn params(&self) -> Vec<ToSqlOutput> {
        vec![
            self.user_id.to_sql().unwrap(),
            self.token.to_sql().unwrap(),
            self.created.to_sql().unwrap(),
            self.expires.to_sql().unwrap(),
        ]
    }
}

impl DbObject for InviteLink {
    fn get_id(&self) -> Id {
        self.id
    }
    fn view_access() -> Access {
        Access::Owner.or(Access::PrivilegedUser)
    }

    fn update_access() -> Access {
        Access::PrivilegedUser
    }

    fn create_access() -> Access {
        Access::PrivilegedUser
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

    fn owner_id_column_index() -> Option<usize> {
        Some(2)
    }

    fn params(&self) -> Vec<ToSqlOutput> {
        vec![
            self.id.to_sql().unwrap(),
            self.invite_token.to_sql().unwrap(),
            self.creator_id.to_sql().unwrap(),
            self.created.to_sql().unwrap(),
        ]
    }
}
