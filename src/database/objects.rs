use crate::database::types::{Access, Column, Id, Token, Type};
use argon2::password_hash::SaltString;
use chrono::{DateTime, Utc};
use rusqlite::types::ToSqlOutput;
use rusqlite::{Row, ToSql};
use serde::{Deserialize, Serialize};
use std::fmt::Debug;

/// An object that is meant to be stored in a database
/// the object must have a unique Id, by default in the first column
#[allow(dead_code)]
pub trait DbObject {
    /// [`Access`] level dictating which users can create the object using th api.
    fn view_access() -> Access;
    /// [`Access`] level for updating and removing of the object.
    fn update_access() -> Access;
    /// [`Access`] level for creating of the object.
    fn create_access() -> Access;
    /// whether a user can create this object using the API
    ///
    /// # Panics
    ///
    /// see [`Access::can_access`]
    fn can_create(user: &User) -> bool {
        Self::create_access().can_access::<Self>(None, user)
    }

    fn table_name() -> &'static str;

    fn columns() -> Vec<Column>;
    /// Returns object's Id
    fn from_row(row: &Row) -> rusqlite::Result<Self>
    where
        Self: Sized;
    fn get_id(&self) -> Id;
    fn id_column_index() -> usize {
        0
    }
    fn get_column(name: &str) -> Option<Column> {
        Self::columns()
            .iter()
            .find(|c| c.name == name)
            .map(|c| c.to_owned())
    }

    fn get_column_index(name: &str) -> Option<usize> {
        Self::columns().iter().position(|c| c.name == name)
    }

    fn database_descriptor() -> String {
        Self::columns()
            .iter()
            .map(|column| format!("{} {}", column.name, column.descriptor()))
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn params(&self) -> Vec<ToSqlOutput>;
}

/// `id`: mod's unique [`Id`]
///
/// `owner_id`: references [`User`]
///
/// `version_id`: references [`Version`]
///
/// `name`: name displayed to the client
///
/// `description`: mod's description
///
/// `icon_id`: id of the icon stored in the filesystem (data/icons)
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct Mod {
    pub id: Id,
    pub owner_id: Id,
    pub version_id: Id,
    pub name: String,
    pub description: String,
    pub icon_id: Option<Id>,
}

/// `id`: version's unique [`Id`]
///
/// `minecraft_version`: version string displayed to the client (like "1.20.1")
///
/// `mod_loader_id`: references [`ModLoader`]
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
#[allow(clippy::struct_field_names)]
pub struct Version {
    pub id: Id,
    pub minecraft_version: String,
    pub mod_loader_id: Id,
}

/// `id`: the mod loader's unique [`Id`]
///
/// `name`: mod loader's name (like "Fabric" or "Vanilla")
///
/// `can_load_mods`: if the mod loader actually can load mods
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct ModLoader {
    pub id: Id,
    pub name: String,
    pub can_load_mods: bool,
}

/// `id`: world's unique [`Id`]
///
/// `owner_id`: references [`User`]
///
/// `name`: world's name
///
/// `icon_id`: id of the icon stored in the filesystem (data/icons)
///
/// `allocated_memory`: amount of memory allocated to the server in MiB
///
/// `version_id`: references [`Version`]
///
/// `enabled`: whether a server hosting this world should be running or not
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct World {
    pub id: Id,
    pub owner_id: Id,
    pub name: String,
    pub icon_id: Option<Id>,
    pub allocated_memory: u32,
    pub version_id: Id,
    pub enabled: bool,
}

/// `id`: user's unique [`Id`]
///
/// `name`: user's unique name
///
/// `memory_limit`: limit of user's total allocatable memory in MiB. [`None`] means no limit
///
/// `player_limit`: per-world player limit. [`None`] means no limit
///
/// `world_limit`: how many worlds can a user create. [`None`] means no limit
///
/// `active_world_limit`: how many worlds can be enabled at a time. [`None`] means no limit
///
/// `storage_limit`: how much storage is available to a user in MiB. [`None`] means no limit
///
/// `is_privileged`: whether a user has administrative privileges, this means they can manage other users and create new accounts
///
/// `enabled`: whether the user can access the API
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

/// `user_id`: unique [`Id`] of the user to whom the password belongs
///
/// `salt`: the [`SaltString`] used for password hashing
///
/// `hash`: hash of the password and the `salt`
#[derive(Debug, PartialEq, Eq, Clone)]
//password is in a separate object to avoid accidentally sending those values together with user data
pub struct Password {
    pub user_id: Id,
    pub salt: SaltString,
    pub hash: String,
}

/// `user_id`: unique [`Id`] of the user who created the session
///
/// `token`: the session [`Token`]
///
/// `created`: [`DateTime`] of when the session was created
///
/// `expires`: whether the session should expire after some time specified in the config after creation
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct Session {
    pub user_id: Id,
    pub token: Token,
    pub created: DateTime<Utc>,
    pub expires: bool,
}

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
    fn get_id(&self) -> Id {
        self.id
    }
    fn params(&self) -> Vec<ToSqlOutput> {
        vec![
            self.id
                .to_sql()
                .expect("failed to convert the value to sql"),
            self.owner_id
                .to_sql()
                .expect("failed to convert the value to sql"),
            self.version_id
                .to_sql()
                .expect("failed to convert the value to sql"),
            self.name
                .to_sql()
                .expect("failed to convert the value to sql"),
            self.description
                .to_sql()
                .expect("failed to convert the value to sql"),
            self.icon_id
                .to_sql()
                .expect("failed to convert the value to sql"),
        ]
    }
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
            Column::new("id", Type::Id).primary_key(),
            Column::new("minecraft_version", Type::Text).not_null(),
            Column::new("mod_loader_id", Type::Id)
                .not_null()
                .references("mod_loaders(id)"),
        ]
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

    fn get_id(&self) -> Id {
        self.id
    }

    fn params(&self) -> Vec<ToSqlOutput> {
        vec![
            self.id
                .to_sql()
                .expect("failed to convert the value to sql"),
            self.minecraft_version
                .to_sql()
                .expect("failed to convert the value to sql"),
            self.mod_loader_id
                .to_sql()
                .expect("failed to convert the value to sql"),
        ]
    }
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

    fn columns() -> Vec<Column> {
        vec![
            Column::new("id", Type::Id).primary_key(),
            Column::new("name", Type::Text).not_null(),
            Column::new("can_load_mods", Type::Boolean)
                .not_null()
                .default("false"),
        ]
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

    fn get_id(&self) -> Id {
        self.id
    }

    fn params(&self) -> Vec<ToSqlOutput> {
        vec![
            self.id
                .to_sql()
                .expect("failed to convert the value to sql"),
            self.name
                .to_sql()
                .expect("failed to convert the value to sql"),
            self.can_load_mods
                .to_sql()
                .expect("failed to convert the value to sql"),
        ]
    }
}

impl DbObject for World {
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
    fn get_id(&self) -> Id {
        self.id
    }
    fn params(&self) -> Vec<ToSqlOutput> {
        vec![
            self.id
                .to_sql()
                .expect("failed to convert the value to sql"),
            self.owner_id
                .to_sql()
                .expect("failed to convert the value to sql"),
            self.name
                .to_sql()
                .expect("failed to convert the value to sql"),
            self.icon_id
                .to_sql()
                .expect("failed to convert the value to sql"),
            self.allocated_memory
                .to_sql()
                .expect("failed to convert the value to sql"),
            self.version_id
                .to_sql()
                .expect("failed to convert the value to sql"),
            self.enabled
                .to_sql()
                .expect("failed to convert the value to sql"),
        ]
    }
}
impl DbObject for User {
    fn view_access() -> Access {
        Access::Owner("id").or(Access::PrivilegedUser)
    }

    //it's this way so the user can't alter their limits. things like username or password changes will use their own access checks
    fn update_access() -> Access {
        Access::PrivilegedUser
    }

    fn create_access() -> Access {
        Access::PrivilegedUser
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

    fn get_id(&self) -> Id {
        self.id
    }

    fn params(&self) -> Vec<ToSqlOutput> {
        vec![
            self.id
                .to_sql()
                .expect("failed to convert the value to sql"),
            self.name
                .to_sql()
                .expect("failed to convert the value to sql"),
            self.avatar_id
                .to_sql()
                .expect("failed to convert the value to sql"),
            self.memory_limit
                .to_sql()
                .expect("failed to convert the value to sql"),
            self.player_limit
                .to_sql()
                .expect("failed to convert the value to sql"),
            self.world_limit
                .to_sql()
                .expect("failed to convert the value to sql"),
            self.active_world_limit
                .to_sql()
                .expect("failed to convert the value to sql"),
            self.storage_limit
                .to_sql()
                .expect("failed to convert the value to sql"),
            self.is_privileged
                .to_sql()
                .expect("failed to convert the value to sql"),
            self.enabled
                .to_sql()
                .expect("failed to convert the value to sql"),
        ]
    }
}

impl Default for User {
    fn default() -> Self {
        Self {
            id: Id::default(),
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
    fn view_access() -> Access {
        Access::None
    }

    fn update_access() -> Access {
        Access::None
    }

    fn create_access() -> Access {
        Access::None
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

    fn get_id(&self) -> Id {
        self.user_id
    }

    fn params(&self) -> Vec<ToSqlOutput> {
        vec![
            self.user_id
                .to_sql()
                .expect("failed to convert the value to sql"),
            self.salt
                .as_str()
                .to_sql()
                .expect("failed to convert the value to sql"),
            self.hash
                .to_sql()
                .expect("failed to convert the value to sql"),
        ]
    }
}

impl DbObject for Session {
    fn view_access() -> Access {
        Access::PrivilegedUser
    }

    fn update_access() -> Access {
        Access::PrivilegedUser
    }

    //TODO: this probably should be creatable by users as well
    fn create_access() -> Access {
        Access::PrivilegedUser
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

    fn get_id(&self) -> Id {
        self.user_id
    }

    fn params(&self) -> Vec<ToSqlOutput> {
        vec![
            self.user_id
                .to_sql()
                .expect("failed to convert the value to sql"),
            self.token
                .to_sql()
                .expect("failed to convert the value to sql"),
            self.created
                .to_sql()
                .expect("failed to convert the value to sql"),
            self.expires
                .to_sql()
                .expect("failed to convert the value to sql"),
        ]
    }
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
