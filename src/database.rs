use crate::database::objects::{DbObject, Group};
use crate::database::objects::{
    InviteLink, Mod, ModLoader, Password, Session, User, Version, World,
};
use crate::database::types::Id;
use crate::execute_on_enum;
use serde::{Deserialize, Deserializer};
use sqlx::{Database as SqlxDatabase, Encode, FromRow, IntoArguments, Pool, Postgres, Type};
use std::error::Error;
use std::fmt::{Debug, Display, Formatter};
use warp::reject::Reject;

pub mod objects;
pub mod types;


#[derive(Debug)]
pub struct Database {
    //pub conn: rusqlite::Connection,
    pub pool: DatabasePool,
}

pub enum DatabaseType {
    Sqlite,
    Postgres,
}

#[allow(dead_code)]
impl Database {
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

        Ok(())
    }

    pub async fn update<
        T: DbObject
            + for<'a> IntoArguments<'a, sqlx::Sqlite>
            + for<'a> IntoArguments<'a, sqlx::Postgres>
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

        value.after_update(self).await?;
        Ok(())
    }

    pub async fn remove<
        T: DbObject
            + for<'r> FromRow<'r, sqlx::sqlite::SqliteRow>
            + for<'r> FromRow<'r, sqlx::postgres::PgRow>
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

#[derive(Debug)]
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
                .map(|(id, column)| { format!("{} = {}", column.name, DB::db_type().nth_parameter(id + 1)) })
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
impl Reject for DatabaseError {}

impl From<sqlx::Error> for DatabaseError {
    fn from(error: sqlx::Error) -> Self {
        match error {
            sqlx::Error::RowNotFound => DatabaseError::NotFound,
            _ => Self::SqlxError(error),
        }
    }
}

/*
#[rustfmt::skip]
#[test]
pub fn manipulate_data() -> anyhow::Result<()> {
    use crate::database::types::Id;
    use chrono::DateTime;
    use argon2::password_hash::SaltString;
    use argon2::password_hash::rand_core::OsRng;
    use pretty_assertions::assert_eq;
    use log::{info};
    use crate::database::types::Token;
    //use crate::database::types::Type::Token;

    let conn = rusqlite::Connection::open_in_memory()?;

    let database = Database { conn };
    database.init()?;

    let mut mod_loader = ModLoader {
        id: Id::default(),
        name: "Mod Loader".to_string(),
        can_load_mods: false,
    };

    let mut version = Version {
        id: Id::default(),
        minecraft_version: "1.2.3.4".to_string(),
        mod_loader_id: mod_loader.id,
    };


    let mut user_min = User {
        id: Id::default(),
        username: String::new(),
        avatar_id: None,
        enabled: false,
    };

    let mut user_max = User {
        id: Id::default(),
        username: "Username".to_string(),
        avatar_id: Some(Id::default()),
        enabled: true,
    };

    let mut password = Password {
        user_id: user_max.id,
        salt: SaltString::generate(OsRng),
        hash: "c4cba546842abb56f76fa61dd51de353c0a3ba1fdf4b9c6e69cdc079eda8e235".to_string(),
    };

    let mut mc_mod_min = Mod {
        id: Id::default(),
        version_id: version.id,
        name: String::new(),
        description: String::new(),
        icon_id: None,
        owner_id: user_max.id,
    };

    let mut mc_mod_max = Mod {
        id: Id::from_string("ABCDEFGH")?,
        version_id: version.id,
        name: "Mod Name".to_string(),
        description: "Mod Description".to_string(),
        icon_id: Some(Id::default()),
        owner_id: user_max.id,
    };

    let mut world_min = World {
        id: Id::default(),
        owner_id: user_min.id,
        name: String::new(),
        hostname: "".to_string(),
        icon_id: None,
        allocated_memory: 0,
        version_id: version.id,
        enabled: false,
    };

    let mut world_max = World {
        id: Id::default(),
        owner_id: user_max.id,
        name: "World Name".to_string(),
        hostname: "hostname".to_string(),
        icon_id: Some(Id::default()),
        allocated_memory: 1024,
        version_id: version.id,
        enabled: true,
    };

    let mut session = Session {
        user_id: user_min.id,
        token: Token::default(),
        created: DateTime::default(),
        expires: false,
    };

    let mut invite_link = InviteLink {
        id: Id::default(),
        invite_token: Token::new(1),
        creator_id: user_min.id,
        created: DateTime::default(),
    };

    database.insert(&mod_loader, None)?;
    database.insert(&version, None)?;
    database.insert(&user_min, None)?;
    database.insert(&user_max, None)?;
    database.insert(&password, None)?;
    database.insert(&mc_mod_min, None)?;
    database.insert(&mc_mod_max, None)?;
    database.insert(&world_min, None)?;
    database.insert(&world_max, None)?;
    database.insert(&session, None)?;
    database.insert(&invite_link, None)?;

    info!("checking inserted objects");
    assert_eq!(mod_loader, database.get_one::<ModLoader>(mod_loader.id, None)?);
    assert_eq!(version, database.get_one::<Version>(version.id, None)?);
    assert_eq!(user_min, database.get_one::<User>(user_min.id, None)?);
    assert_eq!(user_max, database.get_one::<User>(user_max.id, None)?);
    assert_eq!(password, database.get_one::<Password>(password.user_id, None)?);
    assert_eq!(mc_mod_min, database.get_one::<Mod>(mc_mod_min.id, None)?);
    assert_eq!(mc_mod_max, database.get_one::<Mod>(Id::from_string("ABCDEFGH").expect("invalid id"), None)?);
    assert_eq!(world_min, database.get_one::<World>(world_min.id, None)?);
    assert_eq!(world_max, database.get_one::<World>(world_max.id, None)?);
    assert_eq!(session, database.get_one::<Session>(session.user_id, None)?);
    assert_eq!(invite_link, database.get_one::<InviteLink>(invite_link.id, None)?);

    info!("altering values");
    mod_loader.name = "New Display Name".to_string();
    version.minecraft_version = "4.3.2.1".to_string();
    user_min.username = "New Username".to_string();
    user_max.username = "Other New Username".to_string();
    password.salt = SaltString::generate(OsRng);
    mc_mod_min.name = "New Mod Name".to_string();
    mc_mod_max.name = "New Mod Name".to_string();
    world_min.name = "New World Name".to_string();
    world_max.name = "New World Name".to_string();
    session.created = chrono::Utc::now(); //this test will fail on january 1st 1970 (highly unlikely)
    invite_link.invite_token = Token::new(1);

    info!("checking if objects no longer the same");
    assert_ne!(mod_loader, database.get_one::<ModLoader>(mod_loader.id, None)?);
    assert_ne!(version, database.get_one::<Version>(version.id, None)?);
    assert_ne!(user_min, database.get_one::<User>(user_min.id, None)?);
    assert_ne!(user_max, database.get_one::<User>(user_max.id, None)?);
    assert_ne!(password, database.get_one::<Password>(password.user_id, None)?);
    assert_ne!(mc_mod_min, database.get_one::<Mod>(mc_mod_min.id, None)?);
    assert_ne!(mc_mod_max, database.get_one::<Mod>(mc_mod_max.id, None)?);
    assert_ne!(world_min, database.get_one::<World>(world_min.id, None)?);
    assert_ne!(world_max, database.get_one::<World>(world_max.id, None)?);
    assert_ne!(session, database.get_one::<Session>(session.user_id, None)?);
    assert_ne!(invite_link, database.get_one::<InviteLink>(invite_link.id, None)?);

    info!("updating objects");
    assert_eq!(database.update(&mod_loader, None)?, 1);
    assert_eq!(database.update(&version, None)?, 1);
    assert_eq!(database.update(&user_min, None)?, 1);
    assert_eq!(database.update(&user_max, None)?, 1);
    assert_eq!(database.update(&password, None)?, 1);
    assert_eq!(database.update(&mc_mod_min, None)?, 1);
    assert_eq!(database.update(&mc_mod_max, None)?, 1);
    assert_eq!(database.update(&world_min, None)?, 1);
    assert_eq!(database.update(&world_max, None)?, 1);
    assert_eq!(database.update(&session, None)?, 1);
    assert_eq!(database.update(&invite_link, None)?, 1);

    info!("checking if objects are again the same");
    assert_eq!(mod_loader, database.get_one::<ModLoader>(mod_loader.id, None)?);
    assert_eq!(version, database.get_one::<Version>(version.id, None)?);
    assert_eq!(user_min, database.get_one::<User>(user_min.id, None)?);
    assert_eq!(user_max, database.get_one::<User>(user_max.id, None)?);
    assert_eq!(password, database.get_one::<Password>(password.user_id, None)?);
    assert_eq!(mc_mod_min, database.get_one::<Mod>(mc_mod_min.id, None)?);
    assert_eq!(mc_mod_max, database.get_one::<Mod>(mc_mod_max.id, None)?);
    assert_eq!(world_min, database.get_one::<World>(world_min.id, None)?);
    assert_eq!(world_max, database.get_one::<World>(world_max.id, None)?);
    assert_eq!(session, database.get_one::<Session>(session.user_id, None)?);
    assert_eq!(invite_link, database.get_one::<InviteLink>(invite_link.id, None)?);

    info!("removing objects");
    assert_eq!(database.remove(&world_min, None)?, 1);
    assert_eq!(database.remove(&world_max, None)?, 1);
    assert_eq!(database.remove(&mc_mod_min, None)?, 1);
    assert_eq!(database.remove(&mc_mod_max, None)?, 1);
    assert_eq!(database.remove(&version, None)?, 1);
    assert_eq!(database.remove(&mod_loader, None)?, 1);
    assert_eq!(database.remove(&session, None)?, 1);
    assert_eq!(database.remove(&invite_link, None)?, 1);
    assert_eq!(database.remove(&password, None)?, 1);
    assert_eq!(database.remove(&user_min, None)?, 1);
    assert_eq!(database.remove(&user_max, None)?, 1);

    info!("checking if objects are actually removed");
    assert!(database.get_one::<ModLoader>(mod_loader.id, None).is_err());
    assert!(database.get_one::<Version>(version.id, None).is_err());
    assert!(database.get_one::<User>(user_min.id, None).is_err());
    assert!(database.get_one::<User>(user_max.id, None).is_err());
    assert!(database.get_one::<Password>(password.user_id, None).is_err());
    assert!(database.get_one::<Mod>(mc_mod_min.id, None).is_err());
    assert!(database.get_one::<Mod>(mc_mod_max.id, None).is_err());
    assert!(database.get_one::<World>(world_min.id, None).is_err());
    assert!(database.get_one::<World>(world_max.id, None).is_err());
    assert!(database.get_one::<Session>(session.user_id, None).is_err());
    assert!(database.get_one::<InviteLink>(invite_link.id, None).is_err());

    Ok(())
}
 */
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
