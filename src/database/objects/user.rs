use crate::api::handlers::{ApiCreate, ApiGet, ApiList, ApiRemove, ApiUpdate};
use crate::api::util::rejections;
use crate::database::Database;
use crate::database::objects::{DbObject, FromJson, UpdateJson};
use crate::database::types::{Access, Column, Id, Type};
use log::error;
use rusqlite::types::ToSqlOutput;
use rusqlite::{Error, Row, ToSql};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use warp::http::StatusCode;

pub use self::{password::Password, session::Session};

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

    /*
    fn before_delete(&self, database: &Database) {
        for mcmod in database.list_filtered()
        //delete all things that the user
    }
     */

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

#[derive(Debug, Clone, Deserialize)]
pub struct JsonFrom {
    pub name: String,
    pub password: String,
    pub avatar_id: Option<Id>,
    pub memory_limit: Option<u32>,
    pub player_limit: Option<u32>,
    pub world_limit: Option<u32>,
    pub active_world_limit: Option<u32>,
    pub storage_limit: Option<u32>,
    pub is_privileged: Option<bool>,
    pub enabled: Option<bool>,
}

impl FromJson for User {
    type JsonFrom = JsonFrom;
    fn from_json(data: Self::JsonFrom, _user: User) -> Self {
        Self {
            id: Id::default(),
            name: data.name,
            avatar_id: data.avatar_id,
            memory_limit: data.memory_limit,
            player_limit: data.player_limit,
            world_limit: data.world_limit,
            active_world_limit: data.active_world_limit,
            storage_limit: data.storage_limit,
            is_privileged: data.is_privileged.unwrap_or(false),
            enabled: data.enabled.unwrap_or(true),
        }
    }
}

impl UpdateJson for User {
    fn update_with_json(&self, data: Self::JsonFrom) -> Self {
        let mut new = self.clone();
        new.name = data.name;
        new.avatar_id = data.avatar_id;
        new.memory_limit = data.memory_limit;
        new.player_limit = data.player_limit;
        new.world_limit = data.world_limit;
        new.active_world_limit = data.active_world_limit;
        new.storage_limit = data.storage_limit;
        new.is_privileged = data.is_privileged.unwrap_or(new.is_privileged);
        new.enabled = data.enabled.unwrap_or(new.enabled);
        new
    }
}

impl ApiList for User {}
impl ApiGet for User {}
impl ApiCreate for User {
    fn api_create(
        db_mutex: Arc<Mutex<Database>>,
        user: User,
        data: Self::JsonFrom,
    ) -> Result<impl warp::Reply, warp::Rejection> {
        if Self::can_create(&user) {
            db_mutex.lock().map_or_else(
                |err| {
                    Err(warp::reject::custom(rejections::InternalServerError::from(
                        err.to_string(),
                    )))
                },
                |database| match database
                    .create_user_from(Self::from_json(data.clone(), user), &data.password)
                {
                    Ok(new) => Ok(warp::reply::with_status(
                        warp::reply::json(&new),
                        StatusCode::CREATED,
                    )),
                    Err(err) => match err.downcast_ref::<Error>() {
                        Some(err) => {
                            if let Error::SqliteFailure(err, ..) = err {
                                if let rusqlite::ffi::Error {
                                    code: rusqlite::ErrorCode::ConstraintViolation,
                                    ..
                                } = *err
                                {
                                    Ok(warp::reply::with_status(
                                        warp::reply::json(&"username already taken"),
                                        StatusCode::CONFLICT,
                                    ))
                                } else {
                                    error!("{err:?}");
                                    Err(warp::reject::custom(
                                        rejections::InternalServerError::from(err.to_string()),
                                    ))
                                }
                            } else {
                                error!("{err:?}");
                                Err(warp::reject::custom(rejections::InternalServerError::from(
                                    err.to_string(),
                                )))
                            }
                        }
                        _ => Err(warp::reject::custom(rejections::InternalServerError::from(
                            err.to_string(),
                        ))),
                    },
                },
            )
        } else {
            Ok(warp::reply::with_status(
                warp::reply::json(&"Unauthorized"),
                StatusCode::UNAUTHORIZED,
            ))
        }
    }
}
impl ApiUpdate for User {}
impl ApiRemove for User {}

pub mod password {
    use crate::database::objects::DbObject;
    use crate::database::types::{Access, Column, Id, Type};
    use argon2::password_hash::SaltString;
    use rusqlite::types::ToSqlOutput;
    use rusqlite::{Row, ToSql};
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
}

pub mod session {
    use crate::api::handlers::{ApiCreate, ApiGet, ApiList, ApiRemove};
    use crate::database::objects::{DbObject, FromJson, User};
    use crate::database::types::{Access, Column, Id, Token, Type};
    use chrono::{DateTime, Utc};
    use rusqlite::types::ToSqlOutput;
    use rusqlite::{Row, ToSql};
    use serde::{Deserialize, Serialize, Serializer};
    /// `user_id`: unique [`Id`] of the user who created the session
    ///
    /// `token`: the session [`Token`]
    ///
    /// `created`: [`DateTime`] of when the session was created
    ///
    /// `expires`: whether the session should expire after some time specified in the config after creation
    #[derive(Debug, PartialEq, Eq, Clone, Deserialize)]
    pub struct Session {
        pub user_id: Id,
        pub token: Token,
        pub created: DateTime<Utc>,
        pub expires: bool,
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

    #[derive(Debug, Clone, Deserialize)]
    pub struct JsonFrom {
        pub expires: Option<bool>,
    }

    impl FromJson for Session {
        type JsonFrom = JsonFrom;
        fn from_json(data: Self::JsonFrom, user: User) -> Self {
            Self {
                user_id: user.id,
                token: Token::default(),
                created: chrono::offset::Utc::now(),
                expires: data.expires.unwrap_or(true),
            }
        }
    }

    #[derive(Serialize)]
    struct JsonTo {
        user_id: Id,
        created: DateTime<Utc>,
        expires: bool,
    }

    impl Serialize for Session {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            JsonTo {
                user_id: self.user_id,
                created: self.created,
                expires: self.expires,
            }
            .serialize(serializer)
        }
    }

    impl ApiList for Session {}
    impl ApiGet for Session {}
    impl ApiCreate for Session {}
    impl ApiRemove for Session {}
}
