use crate::database::objects::{DbObject, Group};
use crate::database::objects::{
    InviteLink, Mod, ModLoader, Password, Session, User, Version, World,
};
use crate::database::types::{Id, Modifier};
use crate::execute_on_enum;
use log::debug;
use moka::future::Cache;
use serde::{Deserialize, Deserializer, Serialize};
use sqlx::{Database as SqlxDatabase, Encode, FromRow, IntoArguments, Pool, Postgres, Type};
use std::any::Any;
use std::error::Error;
use std::fmt::{Debug, Display, Formatter};
use std::iter::Map;
use std::time::Duration;
use anyhow::{anyhow, bail, Context};
use async_recursion::async_recursion;
use futures::TryFutureExt;
use serde_json::json;
use uuid::Uuid;

pub mod objects;
pub mod types;

#[derive(Debug, Clone)]
pub struct Database {
    //pub conn: rusqlite::Connection,
    pub pool: DatabasePool,
    pub user_cache: Cache<Id, User>,
    pub session_cache: Cache<Uuid, Session>,
    pub group_cache: Cache<Id, Group>,
}

pub enum DatabaseType {
    Sqlite,
    Postgres,
}

#[allow(dead_code)]
impl Database {
    pub fn new(pool: DatabasePool) -> Self {
        let user_cache = Cache::builder()
            .time_to_live(Duration::from_secs(
                crate::config::CONFIG.database.cache_time_to_live,
            ))
            .max_capacity(1000)
            .build();
        let session_cache = Cache::builder()
            .time_to_live(Duration::from_secs(
                crate::config::CONFIG.database.cache_time_to_live,
            ))
            .max_capacity(1000)
            .build();
        let group_cache = Cache::builder()
            .time_to_live(Duration::from_secs(
                crate::config::CONFIG.database.cache_time_to_live,
            ))
            .max_capacity(1000)
            .build();

        Self {
            pool,
            user_cache,
            session_cache,
            group_cache,
        }
    }

    #[rustfmt::skip]
    pub async fn init(&self) -> sqlx::Result<()> {

        execute_on_enum!(&self.pool; (DatabasePool::Postgres, DatabasePool::Sqlite) |pool| {
            sqlx::query(&format!("CREATE TABLE IF NOT EXISTS {} ({});", Group::table_name(),      Group::database_descriptor(&self.db_type()))).execute(pool).await?;
            sqlx::query(&format!("CREATE TABLE IF NOT EXISTS {} ({});", User::table_name(),       User::database_descriptor(&self.db_type()))).execute(pool).await?;
            sqlx::query(&format!("CREATE TABLE IF NOT EXISTS {} ({});", Password::table_name(),   Password::database_descriptor(&self.db_type()))).execute(pool).await?;
            sqlx::query(&format!("CREATE TABLE IF NOT EXISTS {} ({});", Session::table_name(),    Session::database_descriptor(&self.db_type()))).execute(pool).await?;
            sqlx::query(&format!("CREATE TABLE IF NOT EXISTS {} ({});", InviteLink::table_name(), InviteLink::database_descriptor(&self.db_type()))).execute(pool).await?;
            sqlx::query(&format!("CREATE TABLE IF NOT EXISTS {} ({});", ModLoader::table_name(),  ModLoader::database_descriptor(&self.db_type()))).execute(pool).await?;
            sqlx::query(&format!("CREATE TABLE IF NOT EXISTS {} ({});", Version::table_name(),    Version::database_descriptor(&self.db_type()))).execute(pool).await?;
            sqlx::query(&format!("CREATE TABLE IF NOT EXISTS {} ({});", Mod::table_name(),        Mod::database_descriptor(&self.db_type()))).execute(pool).await?;
            sqlx::query(&format!("CREATE TABLE IF NOT EXISTS {} ({});", World::table_name(),      World::database_descriptor(&self.db_type()))).execute(pool).await?;
        });

        Ok(())
    }

    fn db_type(&self) -> DatabaseType {
        match self.pool {
            DatabasePool::Postgres(_) => DatabaseType::Postgres,
            DatabasePool::Sqlite(_) => DatabaseType::Sqlite,
        }
    }

    pub async fn insert<
        T: DbObject
            + for<'a> IntoArguments<'a, sqlx::Sqlite>
            + for<'a> IntoArguments<'a, sqlx::Postgres>
            + Any
            + Clone,
    >(
        &self,
        value: &T,
        user: Option<(&User, &Group)>,
    ) -> Result<(), DatabaseError> {
        if let Some((user, group)) = user {
            if !T::can_create(user, group) {
                return Err(DatabaseError::Unauthorized);
            }
        }
        value.before_create(self).await?;

        execute_on_enum!(&self.pool; (DatabasePool::Postgres, DatabasePool::Sqlite) |pool| {
            let mut query = QueryBuilder::insert(value.clone());
            query
                .query_builder
                .build()
                .execute(pool)
                .await
                .map_err(DatabaseError::from)?;
        });
        value.after_create(self).await?;

        match (value as &dyn Any).downcast_ref::<User>() {
            Some(user) => {
                self.user_cache.insert(user.id, user.clone()).await;
            }
            None => {}
        }
        match (value as &dyn Any).downcast_ref::<Group>() {
            Some(group) => {
                self.group_cache.insert(group.id, group.clone()).await;
            }
            None => {}
        }
        match (value as &dyn Any).downcast_ref::<Session>() {
            Some(session) => {
                self.session_cache
                    .insert(session.token, session.clone())
                    .await;
            }
            None => {}
        }

        Ok(())
    }

    pub async fn update<
        T: DbObject
            + for<'a> IntoArguments<'a, sqlx::Sqlite>
            + for<'a> IntoArguments<'a, sqlx::Postgres>
            + Any
            + Clone,
    >(
        &self,
        value: &T,
        user: Option<(&User, &Group)>,
    ) -> Result<(), DatabaseError> {
        if let Some((user, group)) = user {
            if !value.can_update(user, group) {
                return Err(DatabaseError::Unauthorized);
            }
        }
        value.before_update(self).await?;

        execute_on_enum!(&self.pool; (DatabasePool::Postgres, DatabasePool::Sqlite) |pool| {
            let mut query = QueryBuilder::update(value.clone());

            query.where_id::<T>(value.id());

            if let Some((user, group)) = user {
                query.user_group::<T>(user, group);
            }
            query
                .query_builder
                .build()
                .execute(pool)
                .await
                .map_err(DatabaseError::from)?;
        });

        match (value as &dyn Any).downcast_ref::<User>() {
            Some(user) => {
                self.user_cache.insert(user.id, user.clone()).await;
            }
            None => {}
        }
        match (value as &dyn Any).downcast_ref::<Group>() {
            Some(group) => {
                self.group_cache.insert(group.id, group.clone()).await;
            }
            None => {}
        }
        match (value as &dyn Any).downcast_ref::<Session>() {
            Some(session) => {
                self.session_cache
                    .insert(session.token, session.clone())
                    .await;
            }
            None => {}
        }

        value.after_update(self).await?;
        Ok(())
    }

    pub async fn remove<
        T: DbObject
            + for<'r> FromRow<'r, sqlx::sqlite::SqliteRow>
            + for<'r> FromRow<'r, sqlx::postgres::PgRow>
            + Any
            + Unpin,
    >(
        &self,
        value: &T,
        user: Option<(&User, &Group)>,
    ) -> Result<(), DatabaseError> {
        if let Some((user, group)) = user {
            if !value.can_update(user, group) {
                return Err(DatabaseError::Unauthorized);
            }
        }
        value.before_delete(self).await?;

        match (value as &dyn Any).downcast_ref::<User>() {
            Some(user) => {
                self.user_cache.remove(&user.id).await;
            }
            None => {}
        }
        match (value as &dyn Any).downcast_ref::<Group>() {
            Some(group) => {
                self.group_cache.remove(&group.id).await;
            }
            None => {}
        }
        match (value as &dyn Any).downcast_ref::<Session>() {
            Some(session) => {
                self.session_cache.remove(&session.token).await;
            }
            None => {}
        }

        execute_on_enum!(&self.pool; (DatabasePool::Postgres, DatabasePool::Sqlite) |pool| {
            let mut query = QueryBuilder::delete::<T>();

            query.where_id::<T>(value.id());

            if let Some((user, group)) = user {
                query.user_group::<T>(user, group);
            }
            query
                .query_builder
                .build()
                .execute(pool)
                .await
                .map_err(DatabaseError::from)?;
        });

        value.after_delete(self).await?;
        Ok(())
    }

    pub async fn get_one<
        T: DbObject
            + for<'r> FromRow<'r, sqlx::sqlite::SqliteRow>
            + for<'r> FromRow<'r, sqlx::postgres::PgRow>
            + Unpin,
    >(
        &self,
        id: Id,
        user: Option<(&User, &Group)>,
    ) -> Result<T, DatabaseError> {
        execute_on_enum!(&self.pool; (DatabasePool::Postgres, DatabasePool::Sqlite) |pool| {
            let mut query = QueryBuilder::select::<T>();
            query.where_id::<T>(id);
            if let Some((user, group)) = user {
                query.user_group::<T>(user, group);
            }
            query
                .query_builder
                .build_query_as()
                .fetch_one(pool)
                .await
                .map_err(DatabaseError::from)
        })
    }

    #[async_recursion]
    pub async fn get_recursive<
        T: DbObject
        + for<'r> FromRow<'r, sqlx::sqlite::SqliteRow>
        + for<'r> FromRow<'r, sqlx::postgres::PgRow>
        + Serialize
        + Unpin
    >(
        &self,
        id: Id,
        user: Option<(&User, &Group)>,
    ) -> Result<serde_json::Value, DatabaseError> {
        let object: T = self.get_one::<T>(id, user).map_err(|err| DatabaseError::InternalServerError(err.to_string())).await?;
        let json = serde_json::to_value(object).map_err(|err| DatabaseError::InternalServerError(err.to_string()))?;

        let mut map = serde_json::Map::new();
        if let Some(object) = json.as_object() {
            for (field, value) in object.iter() {
                if let Ok((field, value)) = self.object_from_field::<T>(field, value.clone(), user).await {
                    let field = field.strip_suffix("_id").map(|str| str.to_string()).unwrap_or(field);
                    map.insert(field, value);
                } else {
                    map.insert(field.clone(), value.clone());
                }
            }

            Ok(serde_json::to_value(map).map_err(|err| DatabaseError::InternalServerError(err.to_string()))?)
        } else {
            Ok(json)
        }
    }

    pub async fn get_user(
        &self,
        id: Id,
        user: Option<(&User, &Group)>,
    ) -> Result<User, DatabaseError> {
        let db_user = if let Some(cached_user) = self.user_cache.get(&id).await {
            cached_user
        } else {
            let db_user: User = execute_on_enum!(&self.pool; (DatabasePool::Postgres, DatabasePool::Sqlite) |pool| {
                let mut query = QueryBuilder::select::<User>();
                query.where_id::<User>(id);
                //no user filter in query, it will be done later so it always lands in cache
                query
                    .query_builder
                    .build_query_as()
                    .fetch_one(pool)
                    .await
                    .map_err(DatabaseError::from)
            })?;

            self.user_cache.insert(id, db_user.clone()).await;
            db_user
        };

        if let Some((user, group)) = user {
            if !db_user.viewable_by(user, group) {
                return Err(DatabaseError::NotFound);
            }
        }

        Ok(db_user)
    }

    #[async_recursion]
    pub async fn get_user_recursive (
        &self,
        id: Id,
        user: Option<(&User, &Group)>,
    ) -> Result<serde_json::Value, DatabaseError> {
        let object: User = self.get_user(id, user).map_err(|err| DatabaseError::InternalServerError(err.to_string())).await?;
        let json = serde_json::to_value(object).map_err(|err| DatabaseError::InternalServerError(err.to_string()))?;

        let mut map = serde_json::Map::new();
        if let Some(object) = json.as_object() {
            for (field, value) in object.iter() {
                if let Ok((field, value)) = self.object_from_field::<User>(field, value.clone(), user).await {
                    let field = field.strip_suffix("_id").map(|str| str.to_string()).unwrap_or(field);
                    map.insert(field, value);
                } else {
                    map.insert(field.clone(), value.clone());
                }
            }

            Ok(serde_json::to_value(map).map_err(|err| DatabaseError::InternalServerError(err.to_string()))?)
        } else {
            Ok(json)
        }

    }

    pub async fn get_group(
        &self,
        id: Id,
        user: Option<(&User, &Group)>,
    ) -> Result<Group, DatabaseError> {
        let db_user = if let Some(cached_session) = self.group_cache.get(&id).await {
            cached_session
        } else {
            let group: Group = execute_on_enum!(&self.pool; (DatabasePool::Postgres, DatabasePool::Sqlite) |pool| {
                let mut query = QueryBuilder::select::<Group>();
                query.where_id::<Group>(id);
                //no user filter in query, it will be done later so it always lands in cache
                query
                    .query_builder
                    .build_query_as()
                    .fetch_one(pool)
                    .await
                    .map_err(DatabaseError::from)
            })?;

            self.group_cache.insert(id, group.clone()).await;
            group
        };

        if let Some((user, group)) = user {
            if !db_user.viewable_by(user, group) {
                return Err(DatabaseError::NotFound);
            }
        }

        Ok(db_user)
    }

    #[async_recursion]
    pub async fn get_group_recursive(
        &self,
        id: Id,
        user: Option<(&User, &Group)>,
    ) -> Result<serde_json::Value, DatabaseError> {
        let object: Group = self.get_group(id, user).map_err(|err| DatabaseError::InternalServerError(err.to_string())).await?;
        let json = serde_json::to_value(object).map_err(|err| DatabaseError::InternalServerError(err.to_string()))?;

        let mut map = serde_json::Map::new();
        if let Some(object) = json.as_object() {
            for (field, value) in object.iter() {
                if let Ok((field, value)) = self.object_from_field::<Group>(field, value.clone(), user).await {
                    let field = field.strip_suffix("_id").map(|str| str.to_string()).unwrap_or(field);
                    map.insert(field, value);
                } else {
                    map.insert(field.clone(), value.clone());
                }
            }

            Ok(serde_json::to_value(map).map_err(|err| DatabaseError::InternalServerError(err.to_string()))?)
        } else {
            Ok(json)
        }
    }

    pub async fn get_session(
        &self,
        token: Uuid,
        user: Option<(&User, &Group)>,
    ) -> Result<Session, DatabaseError> {
        let db_user = if let Some(cached_session) = self.session_cache.get(&token).await {
            cached_session
        } else {
            let session: Session = execute_on_enum!(&self.pool; (DatabasePool::Postgres, DatabasePool::Sqlite) |pool| {
                let mut query = QueryBuilder::select::<Session>();
                query.where_("token", token);
                //no user filter in query, it will be done later so it always lands in cache
                query
                    .query_builder
                    .build_query_as()
                    .fetch_one(pool)
                    .await
                    .map_err(DatabaseError::from)
            })?;

            self.session_cache.insert(token, session.clone()).await;

            session
        };

        if let Some((user, group)) = user {
            if !db_user.viewable_by(user, group) {
                return Err(DatabaseError::NotFound);
            }
        }

        Ok(db_user)
    }

    #[async_recursion]
    async fn object_from_field<T: DbObject>(&self, field: &str, value: serde_json::Value, user: Option<(&User, &Group)>) -> Result<(String, serde_json::Value), anyhow::Error> {
        if let Some(column) = T::get_column(field) {
            if let Some(Modifier::References(references)) = column.modifiers.iter().find(|modifier| matches!(modifier, Modifier::References(_))) {
                if let Some(references) = references.strip_suffix("(id)") {
                    let id = Id::from_string(value.as_str().context("not a str")?).context("not an id")?;

                    let referenced = match references {
                        "users" => self.get_user_recursive(id, user).await?,
                        "sessions" => self.get_recursive::<Session>(id, user).await?,
                        "groups" => self.get_group_recursive(id, user).await?,
                        "invite_links" => self.get_recursive::<InviteLink>(id, user).await?,
                        "mod_loaders" => self.get_recursive::<ModLoader>(id, user).await?,
                        "mods" => self.get_recursive::<Mod>(id, user).await?,
                        "versions" => self.get_recursive::<Version>(id, user).await?,
                        "worlds" => self.get_recursive::<World>(id, user).await?,
                        _ => bail!("unknown column: {}", field),
                    };

                    return Ok((field.to_string(), serde_json::to_value(referenced)?));
                }
            }
        }
        bail!("not found")
    }

    pub async fn get_where<
        T: DbObject
            + for<'r> FromRow<'r, sqlx::sqlite::SqliteRow>
            + for<'r> FromRow<'r, sqlx::postgres::PgRow>
            + Unpin,
        V: for<'r> Encode<'r, sqlx::Sqlite>
            + Type<sqlx::Sqlite>
            + for<'r> Encode<'r, sqlx::Postgres>
            + Type<sqlx::Postgres>
            + Clone,
    >(
        &self,
        column: &str,
        value: V,
        user: Option<(&User, &Group)>,
    ) -> Result<T, DatabaseError> {
        execute_on_enum!(&self.pool; (DatabasePool::Postgres, DatabasePool::Sqlite) |pool| {
            let mut query = QueryBuilder::select::<T>();
            query.where_(column, value);
            if let Some((user, group)) = user {
                query.user_group::<T>(user, group);
            }

            query
                .query_builder
                .build_query_as()
                .fetch_one(pool)
                .await
                .map_err(DatabaseError::from)
        })
    }

    pub async fn get_all<
        T: DbObject
            + for<'r> FromRow<'r, sqlx::sqlite::SqliteRow>
            + for<'r> FromRow<'r, sqlx::postgres::PgRow>
            + std::marker::Unpin,
    >(
        &self,
        user: Option<(&User, &Group)>,
    ) -> Result<Vec<T>, DatabaseError> {
        execute_on_enum!(&self.pool; (DatabasePool::Postgres, DatabasePool::Sqlite) |pool| {
            let mut query = QueryBuilder::select::<T>();
            if let Some((user, group)) = user {
                query.user_group::<T>(user, group);
            }
            query
                .query_builder
                .build_query_as()
                .fetch_all(pool)
                .await
                .map_err(DatabaseError::from)
        })
    }

    pub async fn get_all_where<
        T: DbObject
            + for<'r> FromRow<'r, sqlx::sqlite::SqliteRow>
            + for<'r> FromRow<'r, sqlx::postgres::PgRow>
            + Unpin,
        V: for<'r> Encode<'r, sqlx::Sqlite>
            + Type<sqlx::Sqlite>
            + for<'r> Encode<'r, sqlx::Postgres>
            + Type<sqlx::Postgres>
            + Clone,
    >(
        &self,
        column: &str,
        value: V,
        user: Option<(&User, &Group)>,
    ) -> Result<Vec<T>, DatabaseError> {
        let value = value.clone();

        execute_on_enum!(&self.pool; (DatabasePool::Postgres, DatabasePool::Sqlite) |pool| {
            let mut query = QueryBuilder::select::<T>();
            query.where_(column, value);
            if let Some((user, group)) = user {
                query.user_group::<T>(user, group);
            }
            query
                .query_builder
                .build_query_as()
                .fetch_all(pool)
                .await
                .map_err(DatabaseError::from)
        })
    }

    /// This should only be used during testing or during first setup to create an admin account
    pub async fn create_user(&self, username: &str, password: &str) -> anyhow::Result<User> {
        let user = User {
            username: username.to_string(),
            ..Default::default()
        };
        self.create_user_from(user, password).await
    }
    /// This should only be used during testing or during first setup to create an admin account
    pub async fn create_user_from(&self, user: User, password: &str) -> anyhow::Result<User> {
        self.insert(&user, None).await?;

        self.insert(&Password::new(user.id, password), None).await?;

        Ok(user)
    }
}

#[derive(Debug, Clone)]
pub enum DatabasePool {
    Postgres(Pool<sqlx::Postgres>),
    Sqlite(Pool<sqlx::sqlite::Sqlite>),
}

impl From<Pool<sqlx::Postgres>> for DatabasePool {
    fn from(pool: Pool<sqlx::Postgres>) -> Self {
        Self::Postgres(pool)
    }
}

impl From<Pool<sqlx::sqlite::Sqlite>> for DatabasePool {
    fn from(pool: Pool<sqlx::sqlite::Sqlite>) -> Self {
        Self::Sqlite(pool)
    }
}

pub struct QueryBuilder<'a, T: sqlx::Database> {
    pub query_builder: sqlx::QueryBuilder<'a, T>,
    pub query_type: QueryType,
    params: usize,
}

pub trait DbType {
    fn db_type() -> DatabaseType;
}

impl DatabasePool {
    fn db_type(&self) -> DatabaseType {
        match self {
            DatabasePool::Postgres(_) => DatabaseType::Postgres,
            DatabasePool::Sqlite(_) => DatabaseType::Sqlite,
        }
    }
}

impl DbType for QueryBuilder<'_, sqlx::Postgres> {
    fn db_type() -> DatabaseType {
        DatabaseType::Postgres
    }
}

impl DbType for QueryBuilder<'_, sqlx::sqlite::Sqlite> {
    fn db_type() -> DatabaseType {
        DatabaseType::Sqlite
    }
}

impl DbType for sqlx::Sqlite {
    fn db_type() -> DatabaseType {
        DatabaseType::Sqlite
    }
}

impl DbType for sqlx::Postgres {
    fn db_type() -> DatabaseType {
        DatabaseType::Postgres
    }
}

impl DbType for Pool<sqlx::Sqlite> {
    fn db_type() -> DatabaseType {
        DatabaseType::Sqlite
    }
}

impl DbType for Pool<Postgres> {
    fn db_type() -> DatabaseType {
        DatabaseType::Postgres
    }
}

enum QueryType {
    Insert,
    Select,
    Update,
    Delete,
}

pub enum WhereOperand {
    Equal,
    NotEqual,
    GreaterThan,
    GreaterThanOrEqual,
    LessThan,
    LessThanOrEqual,
}

impl Display for WhereOperand {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            WhereOperand::Equal => f.write_str("="),
            WhereOperand::NotEqual => f.write_str("!="),
            WhereOperand::GreaterThan => f.write_str(">"),
            WhereOperand::GreaterThanOrEqual => f.write_str(">="),
            WhereOperand::LessThan => f.write_str("<"),
            WhereOperand::LessThanOrEqual => f.write_str("<="),
        }
    }
}

impl DatabaseType {
    fn nth_parameter(&self, n: usize) -> String {
        match self {
            DatabaseType::Sqlite => {
                format!("?{}", n + 1)
            }
            DatabaseType::Postgres => {
                format!("${}", n + 1)
            }
        }
    }
}

impl<'a, DB: sqlx::Database + DbType> QueryBuilder<'a, DB>
where
    i64: sqlx::Type<DB>,
    for<'r> i64: sqlx::Encode<'r, DB>,
{
    pub fn new(query_builder: sqlx::QueryBuilder<'a, DB>, query_type: QueryType) -> Self {
        Self {
            query_builder,
            query_type,
            params: 0,
        }
    }

    pub fn insert<T: DbObject + IntoArguments<'a, DB>>(value: T) -> Self {
        let query = &format!(
            "INSERT INTO {} ({}) VALUES ({})",
            T::table_name(),
            T::columns()
                .iter()
                .map(|column| column.name.to_string())
                .collect::<Vec<String>>()
                .join(", "),
            T::columns()
                .iter()
                .enumerate()
                .map(|i| DB::db_type().nth_parameter(i.0))
                .collect::<Vec<String>>()
                .join(", ")
        );
        Self {
            query_builder: sqlx::QueryBuilder::with_arguments(query, value),
            params: 0,
            query_type: QueryType::Insert,
        }
    }

    pub fn select<T: DbObject>() -> QueryBuilder<'a, DB> {
        let query = &format!(
            "SELECT {} FROM {}",
            T::columns()
                .iter()
                .map(|column| column.name.clone())
                .collect::<Vec<String>>()
                .join(","),
            T::table_name(),
        );
        Self {
            query_builder: sqlx::QueryBuilder::new(query),
            params: 0,
            query_type: QueryType::Select,
        }
    }

    pub fn update<T: DbObject + IntoArguments<'a, DB>>(value: T) -> QueryBuilder<'a, DB> {
        let query = &format!(
            "UPDATE {} SET {}",
            T::table_name(),
            T::columns()
                .iter()
                .enumerate()
                .map(|(n, column)| {
                    format!("{} = {}", column.name, DB::db_type().nth_parameter(n))
                })
                .collect::<Vec<String>>()
                .join(", "),
        );

        Self {
            query_builder: sqlx::QueryBuilder::with_arguments(query, value),
            params: 0,
            query_type: QueryType::Update,
        }
    }

    pub fn delete<T: DbObject>() -> QueryBuilder<'a, DB> {
        let query = &format!("DELETE FROM {}", T::table_name(),);
        Self {
            query_builder: sqlx::QueryBuilder::new(query),
            params: 0,
            query_type: QueryType::Delete,
        }
    }

    pub fn where_id<T: DbObject>(&mut self, id: Id) {
        self.where_(T::columns()[T::id_column_index()].name(), id)
    }

    pub fn where_operand<F: Type<DB> + Encode<'a, DB> + 'a>(
        &mut self,
        column: &str,
        value: F,
        operator: WhereOperand,
    ) {
        if self.params > 0 {
            self.query_builder
                .push(format!(" AND {} {} ", column, operator.to_string()));
            self.params += 1;
        } else {
            self.query_builder
                .push(format!(" WHERE {} {} ", column, operator.to_string()));
            self.params += 1;
        }
        self.query_builder.push_bind(value);
    }

    pub fn where_<F: Type<DB> + Encode<'a, DB> + 'a>(&mut self, column: &str, value: F) {
        self.where_operand(column, value, WhereOperand::Equal);
    }

    pub fn where_not<F: Type<DB> + Encode<'a, DB> + 'a>(&mut self, column: &str, value: F) {
        self.where_operand(column, value, WhereOperand::NotEqual);
    }

    pub fn where_less_than<F: Type<DB> + Encode<'a, DB> + 'a>(&mut self, column: &str, value: F) {
        self.where_operand(column, value, WhereOperand::LessThan);
    }

    pub fn where_less_than_or_equal<F: Type<DB> + Encode<'a, DB> + 'a>(
        &mut self,
        column: &str,
        value: F,
    ) {
        self.where_operand(column, value, WhereOperand::LessThanOrEqual);
    }

    pub fn where_greater_than<F: Type<DB> + Encode<'a, DB> + 'a>(
        &mut self,
        column: &str,
        value: F,
    ) {
        self.where_operand(column, value, WhereOperand::GreaterThan);
    }

    pub fn where_greater_than_or_equal<F: Type<DB> + Encode<'a, DB> + 'a>(
        &mut self,
        column: &str,
        value: F,
    ) {
        self.where_operand(column, value, WhereOperand::GreaterThanOrEqual);
    }

    pub fn where_null(&mut self, column: &str) {
        if self.params > 0 {
            self.query_builder.push(format!(" AND {} IS NULL ", column));
            self.params += 1;
        } else {
            self.query_builder
                .push(format!(" WHERE {} IS NULL ", column));
            self.params += 1;
        }
    }

    pub fn where_not_null(&mut self, column: &str) {
        if self.params > 0 {
            self.query_builder
                .push(format!(" AND {} IS NOT NULL ", column));
            self.params += 1;
        } else {
            self.query_builder
                .push(format!(" WHERE {} IS NOT NULL ", column));
            self.params += 1;
        }
    }

    pub fn user_group<T: DbObject>(&mut self, user: &User, group: &Group) {
        let access = match self.query_type {
            QueryType::Insert => T::create_access(),
            QueryType::Select => T::view_access(),
            QueryType::Update => T::update_access(),
            QueryType::Delete => T::update_access(),
        };

        if self.params > 0 {
            self.query_builder
                .push(format!(" AND {}", access.access_filter::<T>(user, group)));
            self.params += 1;
        } else {
            self.query_builder
                .push(format!(" WHERE {}", access.access_filter::<T>(user, group)));
            self.params += 1;
        }
    }
}

#[derive(Debug)]
pub enum DatabaseError {
    Unauthorized,
    NotFound,
    Conflict,
    InternalServerError(String),
    SqlxError(sqlx::Error),
}

impl Display for DatabaseError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            DatabaseError::Unauthorized => write!(f, "Unauthorized"),
            DatabaseError::NotFound => write!(f, "NotFound"),
            DatabaseError::InternalServerError(err) => write!(f, "Internal server error: {err}"),
            DatabaseError::SqlxError(err) => write!(f, "Sqlx Error: {err}"),
            DatabaseError::Conflict => write!(f, "Conflict"),
        }
    }
}

impl Error for DatabaseError {}

impl From<sqlx::Error> for DatabaseError {
    fn from(error: sqlx::Error) -> Self {
        match error {
            sqlx::Error::RowNotFound => DatabaseError::NotFound,
            _ => Self::SqlxError(error),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ValueType {
    Integer,
    Float,
    Text,
    Boolean,
    Blob,
    Id,
    Token,
    Datetime,
}
