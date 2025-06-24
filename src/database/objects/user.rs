pub use self::{password::Password, session::Session};
use crate::api::handlers::{ApiCreate, ApiGet, ApiList, ApiObject, ApiRemove, ApiUpdate, DbMutex};
use crate::config::CONFIG;
use crate::database::objects::{DbObject, FromJson, Group, Mod, UpdateJson, World};
use crate::database::types::{Access, Column, Id, Type};
use crate::database::{Database, DatabaseError};
use crate::minecraft::server::ServerConfigLimit;
use async_trait::async_trait;
use futures::future;
use log::{error, info, warn};
use rusqlite::types::ToSqlOutput;
use rusqlite::{Row, ToSql};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use warp::{Filter, Rejection, Reply};
use warp_rate_limit::RateLimitConfig;
use crate::database;

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct User {
    /// user's unique [`Id`]
    pub id: Id,
    /// user's unique name
    pub username: String,
    /// [`Id`] of the avatar stored in the filesystem (data/avatars)
    pub avatar_id: Option<Id>,
    /// which permission [`Group`] does the user belong to
    pub group_id: Id,
    /// whether the user can access the API
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

    // delete passwords and sessions. worlds and mods are handled asynchronously through `before_api_remove()`
    fn before_delete(&self, database: &Database) {
        if let Ok(passwords) = database.get_filtered::<Password>(vec![(String::from("user_id"), self.id.as_i64().to_string())], None/*in theory here the access restriction should be put but i couldn't be bothered with that*/) {
            //no idea why i am iterating here but couldn't hurt can it?
            for password in passwords {
                if let Err(err) = database.remove(&password, None) {
                    error!("{}", err);
                }
            }
        }

        if let Ok(sessions) = database.get_filtered::<Session>(vec![(String::from("user_id"), self.id.as_i64().to_string())], None/*in theory here the access restriction should be put but i couldn't be bothered with that*/) {
            for session in sessions {
                if let Err(err) = database.remove(&session, None) {
                    error!("{}", err);
                }
            }
        }
    }

    fn table_name() -> &'static str {
        "users"
    }

    fn columns() -> Vec<Column> {
        vec![
            Column::new("id", Type::Id).primary_key(),
            Column::new("username", Type::Text).not_null().unique(),
            Column::new("avatar_id", Type::Id),
            Column::new("group_id", Type::Id)
                .not_null()
                .references("groups(id)"),
            Column::new("enabled", Type::Boolean)
                .not_null()
                .default("true"),
        ]
    }
    fn from_row(row: &Row) -> rusqlite::Result<Self>
    where
        Self: Sized,
    {
        Ok(Self {
            id: row.get(0)?,
            username: row.get(1)?,
            avatar_id: row.get(2)?,
            group_id: row.get(3)?,
            enabled: row.get(4)?,
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
            self.username
                .to_sql()
                .expect("failed to convert the value to sql"),
            self.avatar_id
                .to_sql()
                .expect("failed to convert the value to sql"),
            self.group_id
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
            username: String::new(),
            avatar_id: None,
            group_id: CONFIG.user_defaults.group_id,
            enabled: true,
        }
    }
}

// Any value that is present is considered Some value, including null.
fn deserialize_some<'de, T, D>(deserializer: D) -> Result<Option<T>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    Deserialize::deserialize(deserializer).map(Some)
}

#[derive(Debug, Clone, Deserialize)]
pub struct JsonFrom {
    pub username: String,
    pub password: String,
    #[serde(default, deserialize_with = "deserialize_some")]
    pub avatar_id: Option<Option<Id>>,
    pub group_id: Option<Id>,
    pub enabled: Option<bool>,
}

impl FromJson for User {
    type JsonFrom = JsonFrom;
    fn from_json(data: &Self::JsonFrom, _user: &User) -> Self {
        Self {
            id: Id::default(),
            username: data.username.clone(),
            avatar_id: data.avatar_id.unwrap_or(None),
            group_id: data.group_id.unwrap_or(CONFIG.user_defaults.group_id),
            enabled: data.enabled.unwrap_or(true),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct JsonUpdate {
    #[serde(default, deserialize_with = "deserialize_some")]
    pub username: Option<String>,
    #[serde(default, deserialize_with = "deserialize_some")]
    pub password: Option<String>,
    #[serde(default, deserialize_with = "deserialize_some")]
    pub avatar_id: Option<Option<Id>>,
    #[serde(default, deserialize_with = "deserialize_some")]
    pub group_id: Option<Id>,
    #[serde(default, deserialize_with = "deserialize_some")]
    pub enabled: Option<bool>,
}
impl UpdateJson for User {
    type JsonUpdate = JsonUpdate;
    fn update_with_json(&self, data: &Self::JsonUpdate) -> Self {
        let mut new = self.clone();
        // TODO: password change
        new.username = data.username.clone().unwrap_or(new.username);
        new.avatar_id = data.avatar_id.unwrap_or(new.avatar_id);
        new.group_id = data.group_id.unwrap_or(new.group_id);
        new.enabled = data.enabled.unwrap_or(new.enabled);
        new
    }
}

impl ApiObject for User {
    fn filters(
        db_mutex: Arc<Mutex<Database>>,
        rate_limit_config: RateLimitConfig,
    ) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone {
        Self::list_filter(db_mutex.clone(), rate_limit_config.clone())
            .or(Self::get_filter(
                db_mutex.clone(),
                rate_limit_config.clone(),
            ))
            .or(Self::create_filter(
                db_mutex.clone(),
                rate_limit_config.clone(),
            ))
            .or(Self::update_filter(
                db_mutex.clone(),
                rate_limit_config.clone(),
            ))
            .or(Self::remove_filter(
                db_mutex.clone(),
                rate_limit_config.clone(),
            ))
    }
}

impl ApiList for User {}
impl ApiGet for User {}
#[async_trait]
impl ApiCreate for User {
    async fn after_api_create(
        &self,
        database: DbMutex,
        json: &mut Self::JsonFrom,
        _user: &User,
    ) -> Result<(), DatabaseError> {
        database
            .lock()
            .await
            .insert(&Password::new(self.id, &json.password), None)
            .expect("failed to create the password for the user.");
        Ok(())
    }
}
#[async_trait]
impl ApiUpdate for User {
    async fn after_api_update(
        &self,
        database: DbMutex,
        json: &mut Self::JsonUpdate,
        _user: &User,
    ) -> Result<(), DatabaseError> {
        //the the password is first created then recreated so it can handle a missing password entry for the user
        if let Some(password) = json.password.clone() {
            match database.lock().await.get_one::<Password>(self.id, None) {
                Ok(password) => {
                    database
                        .lock()
                        .await
                        .remove(&password, None)
                        .expect("failed to remove the password for the user");
                }
                Err(err) => {
                    warn!("password not found for the user {}: {}", self.username, err);
                }
            }

            database
                .lock()
                .await
                .insert(&Password::new(self.id, &password), None)
                .expect("update the password for the user");
        }
        Ok(())
    }
}
#[async_trait]
impl ApiRemove for User {
    async fn before_api_delete(&self, database: DbMutex, user: &User) -> Result<(), DatabaseError> {
        info!("removing user {}", self.id);
        let worlds = database.lock().await.get_filtered::<World>(vec![(String::from("owner_id"), self.id.as_i64().to_string())], None/*in theory here the access restriction should be put but i couldn't be bothered with that*/)?;
        let worlds_task = async {
            let tasks = worlds.iter().map(|world| async {
                if let Err(err) = world.before_api_delete(database.clone(), user).await { error!{"{err}"}}
                if let Err(err) = database.lock().await.remove(world, None) { error!{"{err}"}}
                if let Err(err) = world.after_api_delete(database.clone(), user).await { error!{"{err}"}}
            });
            future::join_all(tasks).await
        };

        let mods = database.lock().await.get_filtered::<Mod>(vec![(String::from("owner_id"), self.id.as_i64().to_string())], None/*in theory here the access restriction should be put but i couldn't be bothered with that*/)?;
        let mods_task = async {
            let tasks = mods.iter().map(|mcmod| async {
                if let Err(err) = mcmod.before_api_delete(database.clone(), user).await { error!{"{err}"}}
                if let Err(err) = database.lock().await.remove(mcmod, None) { error!{"{err}"}}
                if let Err(err) = mcmod.after_api_delete(database.clone(), user).await { error!{"{err}"}}
            });
            future::join_all(tasks).await
        };

        /*let (worlds_task, mods_task) = */future::join(worlds_task, mods_task).await;

        Ok(())
    }
}

impl User {
    pub async fn group(&self, db_mutex: DbMutex, user: Option<(&User, &Group)>) -> Group {
        db_mutex
            .lock()
            .await
            .get_one(self.group_id, user)
            .expect(&format!("couldn't find group with id {}", self.group_id))
    }
}

pub mod password {
    use crate::database::objects::DbObject;
    use crate::database::types::{Access, Column, Id, Type};
    use argon2::password_hash::SaltString;
    use argon2::password_hash::rand_core::OsRng;
    use argon2::{Argon2, PasswordHasher};
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

    impl Password {
        pub(crate) fn new(user_id: Id, password: &str) -> Self {
            let salt = SaltString::generate(&mut OsRng);
            let argon = Argon2::default();

            Self {
                user_id,
                hash: argon
                    .hash_password(password.as_bytes(), &salt)
                    .expect("could not hash password")
                    .to_string(),
                salt,
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
    use crate::api::handlers::{ApiCreate, ApiGet, ApiList, ApiObject, ApiRemove};
    use crate::database::Database;
    use crate::database::objects::{DbObject, FromJson, User};
    use crate::database::types::{Access, Column, Id, Token, Type};
    use chrono::{DateTime, Utc};
    use rusqlite::types::ToSqlOutput;
    use rusqlite::{Row, ToSql};
    use serde::{Deserialize, Serialize, Serializer};
    use std::sync::Arc;
    use tokio::sync::Mutex;
    use warp::{Filter, Rejection, Reply};
    use warp_rate_limit::RateLimitConfig;

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
            Access::Owner("user_id")
        }

        fn update_access() -> Access {
            Access::PrivilegedUser
        }

        fn create_access() -> Access {
            Access::User
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
        fn from_json(data: &Self::JsonFrom, user: &User) -> Self {
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

    //this is for not leaking the session tokens
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

    impl ApiObject for Session {
        fn filters(
            db_mutex: Arc<Mutex<Database>>,
            rate_limit_config: RateLimitConfig,
        ) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone {
            Self::list_filter(db_mutex.clone(), rate_limit_config.clone())
                .or(Self::get_filter(
                    db_mutex.clone(),
                    rate_limit_config.clone(),
                ))
                .or(Self::create_filter(
                    db_mutex.clone(),
                    rate_limit_config.clone(),
                ))
                .or(Self::remove_filter(
                    db_mutex.clone(),
                    rate_limit_config.clone(),
                ))
        }
    }

    impl ApiList for Session {}
    impl ApiGet for Session {}
    impl ApiCreate for Session {}
    impl ApiRemove for Session {}
}
