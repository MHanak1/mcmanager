use crate::database::types::{Id, Token};
use argon2::password_hash::SaltString;
use chrono::{DateTime, Utc};
use rusqlite::{Connection, Row, ToSql, params};
use serde::{Deserialize, Serialize};
use std::fmt::Debug;

/// An object that is ment to be stored in a database
#[allow(dead_code)]
pub trait DbObject {
    fn get_id(&self) -> Id;

    fn can_access(&self, user: &User) -> bool;
    fn can_update(&self, user: &User) -> bool;
    fn can_create(user: &User) -> bool;

    fn from_row(row: &Row) -> rusqlite::Result<Self>
    where
        Self: Sized;
    fn table_name() -> &'static str;

    fn columns() -> Vec<(&'static str, &'static str)>;

    fn database_descriptor() -> String {
        Self::columns()
            .iter()
            .map(|(name, descriptor)| format!("{name} {descriptor}"))
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn insert_self(&self, conn: &Connection) -> rusqlite::Result<usize>;

    fn update_self(&self, conn: &Connection) -> rusqlite::Result<usize>;
    fn remove_self(&self, conn: &Connection) -> rusqlite::Result<usize> {
        conn.execute(
            &format!(
                "DELETE FROM {} WHERE {} = ?1",
                Self::table_name(),
                Self::columns()[0].0
            ),
            params![self.get_id()],
        )
    }
    fn insert_self_with_params(
        &self,
        conn: &Connection,
        params: &[&dyn ToSql],
    ) -> rusqlite::Result<usize>
    where
        Self: Debug,
    {
        conn.execute(
            &format!(
                "INSERT INTO {} ({}) VALUES ({})",
                Self::table_name(),
                Self::columns()
                    .iter()
                    .map(|(name, _descriptor)| (*name).to_string())
                    .collect::<Vec<String>>()
                    .join(", "),
                Self::columns()
                    .iter()
                    .enumerate()
                    .map(|i| { format!("?{}", i.0 + 1) })
                    .collect::<Vec<String>>()
                    .join(", ")
            ),
            params,
        )
    }

    fn update_self_with_params(
        &self,
        conn: &Connection,
        params: &[&dyn ToSql],
    ) -> rusqlite::Result<usize> {
        conn.execute(
            &format!(
                "UPDATE {} SET {} WHERE {} = {}",
                Self::table_name(),
                Self::columns()
                    .iter()
                    .enumerate()
                    .map(|(id, (column, _descriptor))| { format!("{} = ?{}", column, id + 1) })
                    .collect::<Vec<String>>()
                    .join(", "),
                Self::columns()[0].0,
                self.get_id().as_i64(),
            ),
            params,
        )
    }
    fn get_from_db(conn: &Connection, id: Id) -> rusqlite::Result<Self>
    where
        Self: Sized,
    {
        conn.query_row(
            &format!(
                "SELECT * FROM {} WHERE {} = ?1",
                Self::table_name(),
                Self::columns()[0].0
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

    fn can_access(&self, user: &User) -> bool {
        user.enabled && (user.id == self.owner_id || user.is_privileged)
    }

    fn can_update(&self, user: &User) -> bool {
        user.enabled && (user.id == self.owner_id || user.is_privileged)
    }
    fn can_create(user: &User) -> bool {
        user.enabled
    }

    fn from_row(row: &Row) -> rusqlite::Result<Self>
    where
        Self: Sized,
    {
        Ok(Self {
            id: row.get(0)?,
            version_id: row.get(1)?,
            name: row.get(2)?,
            description: row.get(3)?,
            icon_id: row.get(4)?,
            owner_id: row.get(5)?,
        })
    }

    fn table_name() -> &'static str {
        "mods"
    }
    fn columns() -> Vec<(&'static str, &'static str)> {
        vec![
            ("id", "UNSIGNED BIGINT PRIMARY KEY"),
            (
                "version_id",
                "UNSIGNED BIGINT NOT NULL REFERENCES versions(id)",
            ),
            ("name", "TEXT NOT NULL"),
            ("description", "TEXT NOT NULL"),
            ("icon_id", "UNSIGNED BIGINT"),
            ("owner_id", "UNSIGNED BIGINT NOT NULL REFERENCES users(id)"),
        ]
    }

    fn insert_self(&self, conn: &Connection) -> rusqlite::Result<usize> {
        self.insert_self_with_params(
            conn,
            params![
                self.id,
                self.version_id,
                self.name,
                self.description,
                self.icon_id,
                self.owner_id,
            ],
        )
    }

    fn update_self(&self, conn: &Connection) -> rusqlite::Result<usize> {
        self.update_self_with_params(
            conn,
            params![
                self.id,
                self.version_id,
                self.name,
                self.description,
                self.icon_id,
                self.owner_id,
            ],
        )
    }
}

impl DbObject for Version {
    fn get_id(&self) -> Id {
        self.id
    }
    fn can_access(&self, _user: &User) -> bool {
        true
    }
    fn can_update(&self, user: &User) -> bool {
        user.enabled && user.is_privileged
    }
    fn can_create(user: &User) -> bool {
        user.enabled && user.is_privileged
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

    fn columns() -> Vec<(&'static str, &'static str)> {
        vec![
            ("id", "UNSIGNED BIGINT PRIMARY KEY"),
            ("minecraft_version", "TEXT NOT NULL"),
            (
                "mod_loader_id",
                "UNSIGNED BIGINT NOT NULL REFERENCES mod_loaders(id)",
            ),
        ]
    }

    fn insert_self(&self, conn: &Connection) -> rusqlite::Result<usize> {
        self.insert_self_with_params(
            conn,
            params![self.id, self.minecraft_version, self.mod_loader_id],
        )
    }

    fn update_self(&self, conn: &Connection) -> rusqlite::Result<usize> {
        self.update_self_with_params(
            conn,
            params![self.id, self.minecraft_version, self.mod_loader_id,],
        )
    }
}

impl DbObject for ModLoader {
    fn get_id(&self) -> Id {
        self.id
    }
    fn can_access(&self, user: &User) -> bool {
        user.enabled
    }
    fn can_update(&self, user: &User) -> bool {
        user.enabled && user.is_privileged
    }
    fn can_create(user: &User) -> bool {
        user.enabled && user.is_privileged
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

    fn columns() -> Vec<(&'static str, &'static str)> {
        vec![
            ("id", "UNSIGNED BIGINT PRIMARY KEY"),
            ("name", "TEXT NOT NULL"),
            ("can_load_mods", "BOOLEAN NOT NULL DEFAULT false"),
        ]
    }

    fn insert_self(&self, conn: &Connection) -> rusqlite::Result<usize> {
        self.insert_self_with_params(conn, params![self.id, self.name, self.can_load_mods,])
    }

    fn update_self(&self, conn: &Connection) -> rusqlite::Result<usize> {
        self.update_self_with_params(conn, params![self.id, self.name, self.can_load_mods,])
    }
}

impl DbObject for World {
    fn get_id(&self) -> Id {
        self.id
    }
    fn can_access(&self, user: &User) -> bool {
        user.enabled && (self.owner_id == user.id || user.is_privileged)
    }
    fn can_update(&self, user: &User) -> bool {
        user.enabled && (self.owner_id == user.id || user.is_privileged)
    }
    fn can_create(user: &User) -> bool {
        user.enabled
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
    fn columns() -> Vec<(&'static str, &'static str)> {
        vec![
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
        ]
    }

    fn insert_self(&self, conn: &Connection) -> rusqlite::Result<usize> {
        self.insert_self_with_params(
            conn,
            params![
                self.id,
                self.owner_id,
                self.name,
                self.icon_id,
                self.allocated_memory,
                self.version_id,
                self.enabled,
            ],
        )
    }

    fn update_self(&self, conn: &Connection) -> rusqlite::Result<usize> {
        self.update_self_with_params(
            conn,
            params![
                self.id,
                self.owner_id,
                self.name,
                self.icon_id,
                self.allocated_memory,
                self.version_id,
                self.enabled,
            ],
        )
    }
}
impl DbObject for User {
    fn get_id(&self) -> Id {
        self.id
    }
    fn can_access(&self, user: &User) -> bool {
        user.enabled && (self.id == user.id || user.is_privileged)
    }
    fn can_update(&self, user: &User) -> bool {
        user.enabled && (self.id == user.id || user.is_privileged)
    }
    fn can_create(user: &User) -> bool {
        user.enabled && (user.is_privileged)
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

    fn columns() -> Vec<(&'static str, &'static str)> {
        vec![
            ("id", "UNSIGNED BIGINT PRIMARY KEY"),
            ("name", "TEXT NOT NULL"),
            ("avatar_id", "UNSIGNED BIGINT"),
            ("memory_limit", "UNSIGNED INTEGER"),
            ("player_limit", "UNSIGNED INTEGER"),
            ("world_limit", "UNSIGNED INTEGER"),
            ("active_world_limit", "UNSIGNED INTEGER"),
            ("storage_limit", "UNSIGNED INTEGER"),
            ("is_privileged", "BOOLEAN NOT NULL DEFAULT FALSE"),
            ("enabled", "BOOLEAN NOT NULL DEFAULT TRUE"),
        ]
    }

    fn insert_self(&self, conn: &Connection) -> rusqlite::Result<usize> {
        self.insert_self_with_params(
            conn,
            params![
                self.id,
                self.name,
                self.avatar_id,
                self.memory_limit,
                self.player_limit,
                self.world_limit,
                self.active_world_limit,
                self.storage_limit,
                self.is_privileged,
                self.enabled,
            ],
        )
    }

    fn update_self(&self, conn: &Connection) -> rusqlite::Result<usize> {
        self.update_self_with_params(
            conn,
            params![
                self.id,
                self.name,
                self.avatar_id,
                self.memory_limit,
                self.player_limit,
                self.world_limit,
                self.active_world_limit,
                self.storage_limit,
                self.is_privileged,
                self.enabled,
            ],
        )
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

    fn can_access(&self, _user: &User) -> bool {
        false
    }
    //in theory this could be true, but logic that will update this will bypass this check anyway, so it's safer to set it to false
    fn can_update(&self, user: &User) -> bool {
        false
    }
    fn can_create(user: &User) -> bool {
        false
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

    fn columns() -> Vec<(&'static str, &'static str)> {
        vec![
            (
                "user_id",
                "UNSIGNED BIGINT PRIMARY KEY REFERENCES users(id)",
            ),
            ("salt", "TEXT NOT NULL"),
            ("hash", "TEXT NOT NULL"),
        ]
    }

    fn insert_self(&self, conn: &Connection) -> rusqlite::Result<usize> {
        self.insert_self_with_params(
            conn,
            params![self.user_id, self.salt.to_string(), self.hash,],
        )
    }

    fn update_self(&self, conn: &Connection) -> rusqlite::Result<usize> {
        self.update_self_with_params(
            conn,
            params![self.user_id, self.salt.to_string(), self.hash,],
        )
    }
}

impl DbObject for Session {
    fn get_id(&self) -> Id {
        self.user_id
    }

    fn can_access(&self, user: &User) -> bool {
        user.enabled && (self.user_id == user.id || user.is_privileged)
    }
    fn can_update(&self, user: &User) -> bool {
        user.enabled && user.is_privileged
    }
    fn can_create(user: &User) -> bool {
        user.enabled && (user.is_privileged)
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

    fn columns() -> Vec<(&'static str, &'static str)> {
        vec![
            ("user_id", "UNSIGNED INTEGER REFERENCES users(id)"),
            ("token", "TEXT  PRIMARY KEY"),
            ("created", "DATETIME NOT NULL"),
            ("expires", "BOOLEAN NOT NULL DEFAULT TRUE"),
        ]
    }

    fn insert_self(&self, conn: &Connection) -> rusqlite::Result<usize> {
        self.insert_self_with_params(
            conn,
            params![self.user_id, self.token, self.created, self.expires],
        )
    }

    fn update_self(&self, conn: &Connection) -> rusqlite::Result<usize> {
        self.update_self_with_params(
            conn,
            params![self.user_id, self.token, self.created, self.expires],
        )
    }
}

impl DbObject for InviteLink {
    fn get_id(&self) -> Id {
        self.id
    }
    fn can_access(&self, user: &User) -> bool {
        user.enabled && (self.creator_id == user.id || user.is_privileged)
    }
    fn can_update(&self, user: &User) -> bool {
        user.enabled && user.is_privileged
    }
    fn can_create(user: &User) -> bool {
        user.enabled && user.is_privileged
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

    fn columns() -> Vec<(&'static str, &'static str)> {
        vec![
            ("id", "UNSIGNED BIGINT PRIMARY KEY"),
            ("invite_token", "TEXT NOT NULL"),
            ("creator_id", "INTEGER NOT NULL REFERENCES users(id)"),
            ("created", "DATETIME NOT NULL"),
        ]
    }

    fn insert_self(&self, conn: &Connection) -> rusqlite::Result<usize> {
        self.insert_self_with_params(
            conn,
            params![self.id, self.invite_token, self.creator_id, self.created],
        )
    }

    fn update_self(&self, conn: &Connection) -> rusqlite::Result<usize> {
        self.update_self_with_params(
            conn,
            params![self.id, self.invite_token, self.creator_id, self.created,],
        )
    }
}
