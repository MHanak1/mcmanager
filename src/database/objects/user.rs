pub use self::{password::Password, session::Session};
use crate::api::handlers::{
    ApiCreate, ApiGet, ApiIcon, ApiList, ApiObject, ApiRemove, ApiUpdate,
};
use crate::api::serve::AppState;
use crate::config::CONFIG;
use crate::database;
use crate::database::objects::{DbObject, FromJson, Group, Mod, UpdateJson, World};
use crate::database::types::{Access, Column, Id};
use crate::database::{Cachable, Database, DatabaseError, ValueType};
use async_trait::async_trait;
use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post};
use axum::Router;
use futures::future;
use log::{error, info, warn};
use once_cell::sync::Lazy;
use serde::{Deserialize, Deserializer, Serialize};
use sqlx::{Arguments, FromRow, IntoArguments};
use std::any::Any;

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, FromRow)]
pub struct User {
    /// user's unique [`Id`]
    pub id: Id,
    /// user's unique name
    pub username: String,
    /// which permission [`Group`] does the user belong to
    pub group_id: Id,
    /// how much memory in total does the user have allocated
    pub total_memory_usage: i64,
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
    async fn before_delete(&self, database: &database::Database) -> Result<(), DatabaseError> {
        if let Ok(passwords) = database.get_all_where::<Password, _>("user_id", self.id, None/*in theory here the access restriction should be put but i couldn't be bothered with that*/).await {
            //no idea why i am iterating here but couldn't hurt can it?
            for password in passwords {
                database.remove(&password, None).await?;
            }
        }

        if let Ok(sessions) = database.get_all_where::<Session, _>("user_id", self.id, None/*in theory here the access restriction should be put but i couldn't be bothered with that*/).await {
            for session in sessions {
                database.remove(&session, None).await?;
            }
        }
        Ok(())
    }

    fn table_name() -> &'static str {
        "users"
    }

    const COLUMNS: Lazy<Vec<Column>> = Lazy::new(|| {
        vec![
            Column::new("id", ValueType::Id).primary_key(),
            Column::new("username", ValueType::Text).not_null().unique(),
            Column::new("group_id", ValueType::Id)
                .not_null()
                .references("groups(id)"),
            Column::new("total_memory_usage", ValueType::Integer)
                .not_null()
                .default("0"),
            Column::new("enabled", ValueType::Boolean)
                .not_null()
                .default("true"),
        ]
    });

    fn id(&self) -> Id {
        self.id
    }

    fn owner_id(&self) -> Option<Id> {
        Some(self.id)
    }
}

impl Cachable for User {
    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self as Box<dyn Any>
    }
}

impl<'a> IntoArguments<'a, sqlx::Sqlite> for User {
    fn into_arguments(self) -> sqlx::sqlite::SqliteArguments<'a> {
        let mut arguments = sqlx::sqlite::SqliteArguments::default();
        arguments.add(self.id).expect("Failed to argument");
        arguments.add(self.username).expect("Failed to argument");
        arguments.add(self.group_id).expect("Failed to argument");
        arguments
            .add(self.total_memory_usage)
            .expect("Failed to argument");
        arguments.add(self.enabled).expect("Failed to argument");
        arguments
    }
}

impl<'a> IntoArguments<'a, sqlx::Postgres> for User {
    fn into_arguments(self) -> sqlx::postgres::PgArguments {
        let mut arguments = sqlx::postgres::PgArguments::default();
        arguments.add(self.id).expect("Failed to argument");
        arguments.add(self.username).expect("Failed to argument");
        arguments.add(self.group_id).expect("Failed to argument");
        arguments
            .add(self.total_memory_usage)
            .expect("Failed to argument");
        arguments.add(self.enabled).expect("Failed to argument");
        arguments
    }
}

impl Default for User {
    fn default() -> Self {
        Self {
            id: Id::default(),
            username: String::new(),
            group_id: CONFIG.user_defaults.group_id,
            total_memory_usage: 0,
            enabled: true,
        }
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
    pub username: String,
    pub password: String,
    #[serde(default, deserialize_with = "deserialize_some")]
    pub group_id: Option<Id>,
    pub enabled: Option<bool>,
}

impl FromJson for User {
    type JsonFrom = JsonFrom;
    fn from_json(data: &Self::JsonFrom, _user: &User) -> Self {
        Self {
            id: Id::default(),
            username: data.username.clone(),
            group_id: data.group_id.unwrap_or(CONFIG.user_defaults.group_id),
            total_memory_usage: 0,
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
        new.group_id = data.group_id.unwrap_or(new.group_id);
        new.enabled = data.enabled.unwrap_or(new.enabled);
        new
    }
}

impl ApiObject for User {
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

impl ApiList for User {}
impl ApiGet for User {}
#[async_trait]
impl ApiCreate for User {
    async fn after_api_create(
        &self,
        state: AppState,
        json: &mut Self::JsonFrom,
        _user: &User,
    ) -> Result<(), DatabaseError> {
        state
            .database
            .insert(&Password::new(self.id, &json.password), None)
            .await
            .expect("failed to create the password for the user.");
        Ok(())
    }
}
#[async_trait]
impl ApiUpdate for User {
    async fn after_api_update(
        &self,
        state: AppState,
        json: &mut Self::JsonUpdate,
        _user: &User,
    ) -> Result<(), DatabaseError> {
        //the the password is first created then recreated so it can handle a missing password entry for the user
        if let Some(password) = json.password.clone() {
            match state.database.get_one::<Password>(self.id, None).await {
                Ok(password) => {
                    state
                        .database
                        .remove(&password, None)
                        .await
                        .expect("failed to remove the password for the user");
                }
                Err(err) => {
                    warn!("password not found for the user {}: {}", self.username, err);
                }
            }

            state
                .database
                .insert(&Password::new(self.id, &password), None)
                .await
                .expect("update the password for the user");
        }
        Ok(())
    }
}
#[async_trait]
impl ApiRemove for User {
    async fn before_api_delete(&self, state: AppState, user: &User) -> Result<(), DatabaseError> {
        info!("removing user {}", self.id);
        let worlds = state.database.get_all_where::<World, _>("owner_id", self.id, None/*in theory here the access restriction should be put but i couldn't be bothered with that*/).await?;
        let worlds_task = async {
            let tasks = worlds.iter().map(|world| async {
                if let Err(err) = world.before_api_delete(state.clone(), user).await {
                    error! {"{err}"}
                }
                if let Err(err) = state.database.remove(world, None).await {
                    error! {"{err}"}
                }
                if let Err(err) = world.after_api_delete(state.clone(), user).await {
                    error! {"{err}"}
                }
            });
            future::join_all(tasks).await
        };

        let mods = state.database.get_all_where::<Mod, _>("owner_id", self.id, None/*in theory here the access restriction should be put but i couldn't be bothered with that*/).await?;
        let mods_task = async {
            let database = state.clone();
            let tasks = mods.iter().map(|mcmod| async {
                let state = database.clone();
                if let Err(err) = mcmod.before_api_delete(state.clone(), user).await {
                    error! {"{err}"}
                }
                if let Err(err) = state.database.remove(mcmod, None).await {
                    error! {"{err}"}
                }
                if let Err(err) = mcmod.after_api_delete(state, user).await {
                    error! {"{err}"}
                }
            });
            future::join_all(tasks).await
        };

        /*let (worlds_task, mods_task) = */
        future::join(worlds_task, mods_task).await;

        Ok(())
    }
}

impl ApiIcon for User {
    const DEFAULT_ICON_BYTES: &'static [u8] = include_bytes!("../../resources/icons/user_default.png");
    const DEFAULT_ICON_MIME: &'static str = "image/png";
}

impl User {
    pub async fn group(&self, database: Database, user: Option<(&User, &Group)>) -> Group {
        database
            .get_one::<Group>(self.group_id, user)
            .await
            .unwrap_or_else(|_| panic!("couldn't find group with id {}", self.group_id))
    }
}

pub mod password {
    use crate::database::objects::DbObject;
    use crate::database::types::{Access, Column, Id};
    use crate::database::{Cachable, ValueType};
    use argon2::password_hash::rand_core::OsRng;
    use argon2::password_hash::{PasswordHashString, SaltString};
    use argon2::{Argon2, PasswordHasher};
    use duplicate::duplicate_item;
    use once_cell::sync::Lazy;
    use sqlx::{Arguments, Error, FromRow, IntoArguments, Row};
    use std::any::Any;
    use std::str::FromStr;

    /// `user_id`: unique [`Id`] of the user to whom the password belongs
    ///
    /// `hash`: hash of the password and the `salt`
    #[derive(Debug, PartialEq, Eq, Clone)]
    //password is in a separate object to avoid accidentally sending those values together with user data
    pub struct Password {
        pub user_id: Id,
        pub hash: PasswordHashString,
    }

    impl Password {
        pub(crate) fn new(user_id: Id, password: &str) -> Self {
            let salt = SaltString::generate(&mut OsRng);
            let argon = Argon2::default();

            Self {
                user_id,
                hash: PasswordHashString::from(
                    argon
                        .hash_password(password.as_bytes(), &salt)
                        .expect("could not hash password"),
                ),
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

        const COLUMNS: Lazy<Vec<Column>> = Lazy::new(|| {
            vec![
                Column::new("user_id", ValueType::Id)
                    .primary_key()
                    .references("users(id)"),
                Column::new("hash", ValueType::Text).not_null().hidden(),
            ]
        });

        fn id(&self) -> Id {
            self.user_id
        }

        fn owner_id(&self) -> Option<Id> {
            Some(self.user_id)
        }
    }
    impl Cachable for Password {
        fn into_any(self: Box<Self>) -> Box<dyn Any> {
            self as Box<dyn Any>
        }
    }

    impl<'a> IntoArguments<'a, sqlx::Sqlite> for Password {
        fn into_arguments(self) -> sqlx::sqlite::SqliteArguments<'a> {
            let mut arguments = sqlx::sqlite::SqliteArguments::default();
            arguments.add(self.user_id).expect("Failed to add argument");
            arguments
                .add(self.hash.to_string())
                .expect("Failed to add argument");
            arguments
        }
    }

    impl<'a> IntoArguments<'a, sqlx::Postgres> for Password {
        fn into_arguments(self) -> sqlx::postgres::PgArguments {
            let mut arguments = sqlx::postgres::PgArguments::default();
            arguments.add(self.user_id).expect("Failed to add argument");
            arguments
                .add(self.hash.to_string())
                .expect("Failed to add argument");
            arguments
        }
    }

    #[duplicate_item(Row; [sqlx::sqlite::SqliteRow]; [sqlx::postgres::PgRow])]
    impl FromRow<'_, Row> for Password {
        fn from_row(row: &'_ Row) -> Result<Self, Error> {
            Ok(Self {
                user_id: row.get(0),
                hash: PasswordHashString::from_str(row.get(1)).map_err(|err| {
                    sqlx::Error::ColumnDecode {
                        index: "hash".to_string(),
                        source: Box::new(err),
                    }
                })?,
            })
        }
    }
}

pub mod session {
    
    use crate::api::handlers::{
        ApiCreate, ApiGet, ApiList, ApiObject, ApiRemove,
    };
    use crate::api::serve::AppState;
    use crate::database::objects::{DbObject, FromJson, User};
    use crate::database::types::{Access, Column, Id};
    use crate::database::{Cachable, ValueType};
    
    use axum::Router;
    
    
    use axum::routing::get;
    use chrono::{DateTime, Utc};
    use duplicate::duplicate_item;
    use once_cell::sync::Lazy;
    use serde::{Deserialize, Serialize, Serializer};
    use sqlx::{Arguments, Error, FromRow, IntoArguments, Row};
    use std::any::Any;
    
    
    use uuid::Uuid;

    /// `user_id`: unique [`Id`] of the user who created the session
    ///
    /// `token`: the session [`Token`]
    ///
    /// `created`: [`DateTime`] of when the session was created
    ///
    /// `expires`: whether the session should expire after some time specified in the config after creation
    #[derive(Debug, PartialEq, Eq, Clone, Deserialize)]
    pub struct Session {
        pub id: Id,
        pub user_id: Id,
        pub token: Uuid,
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

        const COLUMNS: Lazy<Vec<Column>> = Lazy::new(|| {
            vec![
                Column::new("id", ValueType::Id).unique(),
                Column::new("user_id", ValueType::Id).references("users(id)"),
                Column::new("token", ValueType::Token)
                    .primary_key()
                    .hidden(),
                Column::new("created", ValueType::Datetime).not_null(),
                Column::new("expires", ValueType::Boolean)
                    .not_null()
                    .default("true"),
            ]
        });

        fn id(&self) -> Id {
            self.id
        }

        fn owner_id(&self) -> Option<Id> {
            Some(self.user_id)
        }
    }

    impl Cachable for Session {
        fn into_any(self: Box<Self>) -> Box<dyn Any> {
            self as Box<dyn Any>
        }
    }

    #[duplicate_item(Row; [sqlx::sqlite::SqliteRow]; [sqlx::postgres::PgRow])]
    impl<'r> FromRow<'r, Row> for Session {
        fn from_row(row: &'r Row) -> Result<Self, Error> {
            Ok(Self {
                id: row.get(0),
                user_id: row.get(1),
                token: row.get(2),
                created: row.get(3),
                expires: row.get(4),
            })
        }
    }

    impl<'a> IntoArguments<'a, sqlx::Sqlite> for Session {
        fn into_arguments(self) -> sqlx::sqlite::SqliteArguments<'a> {
            let mut arguments = sqlx::sqlite::SqliteArguments::default();
            arguments.add(self.id).expect("Failed to add argument");
            arguments.add(self.user_id).expect("Failed to add argument");
            arguments.add(self.token).expect("Failed to add argument");
            arguments
                //.add(self.created.to_string())
                .add(self.created)
                .expect("Failed to add argument");
            arguments.add(self.expires).expect("Failed to add argument");
            arguments
        }
    }

    impl<'a> IntoArguments<'a, sqlx::Postgres> for Session {
        fn into_arguments(self) -> sqlx::postgres::PgArguments {
            let mut arguments = sqlx::postgres::PgArguments::default();
            arguments.add(self.id).expect("Failed to add argument");
            arguments.add(self.user_id).expect("Failed to add argument");
            arguments.add(self.token).expect("Failed to add argument");
            arguments.add(self.created).expect("Failed to add argument");
            arguments.add(self.expires).expect("Failed to add argument");
            arguments
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
                id: Id::new_random(),
                user_id: user.id,
                token: Uuid::new_v4(),
                created: chrono::offset::Utc::now(),
                expires: data.expires.unwrap_or(true),
            }
        }
    }

    #[derive(Serialize)]
    struct JsonTo {
        id: Id,
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
                id: self.id,
                user_id: self.user_id,
                created: self.created,
                expires: self.expires,
            }
            .serialize(serializer)
        }
    }

    impl ApiObject for Session {
        fn routes() -> Router<AppState> {
            Router::new()
                .route("/", get(Self::api_list).post(Self::api_create))
                .route("/{id}", get(Self::api_get).delete(Self::api_remove))
        }
    }

    impl ApiList for Session {}
    impl ApiGet for Session {}
    impl ApiCreate for Session {}
    impl ApiRemove for Session {}
}
