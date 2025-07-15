use crate::api::handlers::PaginationSettings;
use crate::database::objects::{DbObject, Group};
use crate::database::objects::{
    InviteLink, Mod, ModLoader, Password, Session, User, Version, World,
};
use crate::database::types::{Id, Modifier};
use crate::execute_on_enum;
use async_recursion::async_recursion;
use color_eyre::Context;
use dyn_clone::DynClone;
use futures::TryFutureExt;
use log::{debug, error, info};
use moka::future::Cache;
use serde::{Deserializer, Serialize};
use serde_json::Value;
use sqlx::{Encode, FromRow, IntoArguments, Pool, Postgres, Type};
use std::any::Any;
use std::collections::HashMap;
use std::error::Error;
use std::fmt::{Debug, Display, Formatter};
use std::sync::Arc;
use std::time::Duration;
use tokio_stream::StreamExt;
use uuid::Uuid;

pub mod objects;
pub mod types;

pub trait Cachable: DynClone + Sync + Send + Any {
    fn into_any(self: Box<Self>) -> Box<dyn Any>;
}

dyn_clone::clone_trait_object!(Cachable);

#[derive(Clone)]
pub struct DatabaseCache {
    pub caches: Arc<HashMap<&'static str, Cache<Id, Box<dyn Cachable>>>>,
}

impl Default for DatabaseCache {
    fn default() -> Self {
        Self::new()
    }
}

impl DatabaseCache {
    pub fn new() -> Self {
        let mut caches = HashMap::new();

        const CACHES_SIZE: u64 = 1000;

        caches.insert(Group::table_name(), Cache::new(CACHES_SIZE));
        caches.insert(InviteLink::table_name(), Cache::new(CACHES_SIZE));
        caches.insert(ModLoader::table_name(), Cache::new(CACHES_SIZE));
        caches.insert(Mod::table_name(), Cache::new(CACHES_SIZE));
        caches.insert(User::table_name(), Cache::new(CACHES_SIZE));
        caches.insert(Session::table_name(), Cache::new(CACHES_SIZE));
        caches.insert(Password::table_name(), Cache::new(CACHES_SIZE));
        caches.insert(Version::table_name(), Cache::new(CACHES_SIZE));
        caches.insert(World::table_name(), Cache::new(CACHES_SIZE));

        for key in caches.keys() {
            println!("{key}")
        }

        Self {
            caches: Arc::new(caches),
        }
    }

    fn get_cache(&self, name: &str) -> Cache<Id, Box<dyn Cachable>> {
        self.caches.get(name).cloned().expect("cache not found")
    }

    pub async fn get<T: DbObject + 'static>(&self, id: Id) -> Option<T> {
        let cache = self.get_cache(&T::table_name());
        let value = cache.get(&id).await?;

        let value = value.into_any().downcast::<T>();

        match value {
            Ok(value) => Some(*value),
            Err(_) => {
                error!("Cache type mismatch");
                None
            }
        }
    }

    pub async fn insert<T: DbObject + Cachable + 'static>(&self, value: T) {
        let cache = self.get_cache(T::table_name());
        cache
            .insert(value.id(), Box::new(value) as Box<dyn Cachable>)
            .await;
    }

    pub async fn insert_all<T: DbObject + Cachable + 'static>(&self, values: Vec<T>) {
        for value in values {
            self.insert(value).await;
        }
    }

    pub async fn remove<T: DbObject>(&self, id: Id) {
        if self.get_cache(T::table_name())
            .remove(&id)
            .await
            .is_none() {
            debug!("value from {} with id {} not in cache, cannot remove", T::table_name(), id);
        }
    }
}

#[derive(Clone)]
pub struct Database {
    //pub conn: rusqlite::Connection,
    pub pool: DatabasePool,
    pub cache: DatabaseCache,
    pub session_cache: Cache<Uuid, Session>,
}

pub enum DatabaseType {
    Sqlite,
    Postgres,
}

#[allow(dead_code)]
impl Database {
    pub fn new(pool: DatabasePool) -> Self {
        let session_cache = Cache::builder()
            .time_to_live(Duration::from_secs(
                crate::config::CONFIG.database.cache_time_to_live,
            ))
            .max_capacity(1000)
            .build();

        Self {
            pool,
            cache: DatabaseCache::new(),
            session_cache,
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
            + Clone
            + Cachable,
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

        self.cache.insert(value.clone()).await;

        if let Some(session) = (value as &dyn Any).downcast_ref::<Session>() {
            self.session_cache
                .insert(session.token, session.clone())
                .await;
        }

        Ok(())
    }

    pub async fn update<
        T: DbObject
            + for<'a> IntoArguments<'a, sqlx::Sqlite>
            + for<'a> IntoArguments<'a, sqlx::Postgres>
            + Any
            + Clone
            + Cachable,
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

        self.cache.insert(value.clone()).await;

        if let Some(session) = (value as &dyn Any).downcast_ref::<Session>() {
            self.session_cache
                .insert(session.token, session.clone())
                .await;
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

        self.cache.remove::<T>(value.id()).await;

        if let Some(session) = (value as &dyn Any).downcast_ref::<Session>() {
            self.session_cache.remove(&session.token).await;
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
            + Unpin
            + Cachable,
    >(
        &self,
        id: Id,
        user: Option<(&User, &Group)>,
    ) -> Result<T, DatabaseError> {
        let db_object = if let Some(cached_object) = self.cache.get(id).await {
            cached_object
        } else {
            let db_object: T = execute_on_enum!(&self.pool; (DatabasePool::Postgres, DatabasePool::Sqlite) |pool| {
                let mut query = QueryBuilder::select::<T>();
                query.where_id::<T>(id);
                //no user filter in query, it will be done later so it always lands in cache
                query
                    .query_builder
                    .build_query_as()
                    .fetch_one(pool)
                    .await
                    .map_err(DatabaseError::from)
            })?;

            self.cache.insert(db_object.clone()).await;
            db_object
        };

        if let Some((user, group)) = user {
            if !db_object.viewable_by(user, group) {
                return Err(DatabaseError::NotFound);
            }
        }

        Ok(db_object)
    }

    #[async_recursion]
    pub async fn get_recursive<
        T: DbObject
            + Cachable
            + for<'r> FromRow<'r, sqlx::sqlite::SqliteRow>
            + for<'r> FromRow<'r, sqlx::postgres::PgRow>
            + Serialize
            + Unpin,
    >(
        &self,
        id: Id,
        user: Option<(&User, &Group)>,
    ) -> Result<serde_json::Value, DatabaseError> {
        let start = tokio::time::Instant::now();
        let object: T = self
            .get_one::<T>(id, user)
            .map_err(|err| DatabaseError::InternalServerError(err.to_string()))
            .await?;
        let json = serde_json::to_value(object)
            .map_err(|err| DatabaseError::InternalServerError(err.to_string()))?;

        let mut map = serde_json::Map::new();
        if let Value::Object(object) = json {
            for (field, value) in object.into_iter() {
                if let Ok((field, value)) = self.object_from_field::<T>(&field, &value, user).await
                {
                    let field = field
                        .strip_suffix("_id")
                        .map(|str| str.to_string())
                        .unwrap_or(field);
                    map.insert(field, value);
                } else {
                    map.insert(field, value);
                }
            }

            Ok(serde_json::to_value(map)
                .map_err(|err| DatabaseError::InternalServerError(err.to_string()))?)
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
    async fn object_from_field<T: DbObject>(
        &self,
        field: &str,
        value: &Value,
        user: Option<(&User, &Group)>,
    ) -> Result<(String, Value), DatabaseError> {
        if let Some(column) = T::get_column(field) {
            if let Some(Modifier::References(references)) = column
                .modifiers
                .iter()
                .find(|modifier| matches!(modifier, Modifier::References(_)))
            {
                if let Some(references) = references.strip_suffix("(id)") {
                    let str = value.as_str();
                    if str.is_none() {
                        return Ok((field.to_string(), value.clone()));
                    }
                    let str = str.unwrap();
                    let id = Id::from_string(str);
                    if id.is_err() {
                        return Ok((field.to_string(), value.clone()));
                    }
                    let id = id.unwrap();

                    let referenced = match references {
                        "users" => self.get_recursive::<User>(id, user).await?,
                        "sessions" => self.get_recursive::<Session>(id, user).await?,
                        "groups" => self.get_recursive::<Group>(id, user).await?,
                        "invite_links" => self.get_recursive::<InviteLink>(id, user).await?,
                        "mod_loaders" => self.get_recursive::<ModLoader>(id, user).await?,
                        "mods" => self.get_recursive::<Mod>(id, user).await?,
                        "versions" => self.get_recursive::<Version>(id, user).await?,
                        "worlds" => self.get_recursive::<World>(id, user).await?,
                        _ => Err(DatabaseError::InternalServerError("Not Found".to_string()))?,
                    };

                    return Ok((field.to_string(), serde_json::to_value(referenced).unwrap()));
                }
            }
        }
        Ok((field.to_string(), value.clone()))
    }

    pub async fn get_where<
        T: DbObject
            + for<'r> FromRow<'r, sqlx::sqlite::SqliteRow>
            + for<'r> FromRow<'r, sqlx::postgres::PgRow>
            + Unpin
            + Cachable,
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
            + Unpin
            + Cachable,
    >(
        &self,
        user: Option<(&User, &Group)>,
    ) -> Result<Vec<T>, DatabaseError> {
        let value = execute_on_enum!(&self.pool; (DatabasePool::Postgres, DatabasePool::Sqlite) |pool| {
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
        })?;

        self.cache.insert_all(value.clone()).await;

        Ok(value)
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
    pub async fn create_user(&self, username: &str, password: &str) -> color_eyre::Result<User> {
        let user = User {
            username: username.to_string(),
            ..Default::default()
        };
        self.create_user_from(user, password).await
    }
    /// This should only be used during testing or during first setup to create an admin account
    pub async fn create_user_from(&self, user: User, password: &str) -> color_eyre::Result<User> {
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

pub enum QueryType {
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
                .map(|column| column.name.to_string())
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

    pub fn pagination<T: DbObject>(&mut self, pagination: PaginationSettings) {
        self.query_builder.push(format!(
            " LIMIT {} OFFSET {}",
            pagination.limit,
            pagination.page * pagination.limit
        ));
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
