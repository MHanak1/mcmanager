use crate::database::objects::DbObject;
use crate::database::objects::{
    InviteLink, Mod, ModLoader, Password, Session, User, Version, World,
};
use crate::database::types::{Id, Type};
use log::debug;
use rusqlite::{params, params_from_iter};
use std::error::Error;
use std::fmt::{Debug, Display, Formatter};
use test_log::test;
use warp::reject::Reject;

pub mod objects;
pub mod types;

#[derive(Debug)]
pub struct Database {
    pub conn: rusqlite::Connection,
}

#[allow(dead_code)]
impl Database {
    #[rustfmt::skip]
    pub fn init(&self) -> rusqlite::Result<()> {
        self.conn.execute(&format!("CREATE TABLE IF NOT EXISTS {} ({});", ModLoader::table_name(),  ModLoader::database_descriptor()), ())?;
        self.conn.execute(&format!("CREATE TABLE IF NOT EXISTS {} ({});", Version::table_name(),    Version::database_descriptor()), ())?;
        self.conn.execute(&format!("CREATE TABLE IF NOT EXISTS {} ({});", Mod::table_name(),        Mod::database_descriptor()), ())?;
        self.conn.execute(&format!("CREATE TABLE IF NOT EXISTS {} ({});", User::table_name(),       User::database_descriptor()), ())?;
        self.conn.execute(&format!("CREATE TABLE IF NOT EXISTS {} ({});", Password::table_name(),   Password::database_descriptor()), ())?;
        self.conn.execute(&format!("CREATE TABLE IF NOT EXISTS {} ({});", World::table_name(),      World::database_descriptor()), ())?;
        self.conn.execute(&format!("CREATE TABLE IF NOT EXISTS {} ({});", Session::table_name(),    Session::database_descriptor()), ())?;
        self.conn.execute(&format!("CREATE TABLE IF NOT EXISTS {} ({});", InviteLink::table_name(), InviteLink::database_descriptor()), ())?;

        Ok(())
    }

    pub fn insert<T: DbObject>(
        &self,
        value: &T,
        user: Option<&User>,
    ) -> Result<usize, DatabaseError> {
        if let Some(user) = user {
            if !T::can_create(user) {
                return Err(DatabaseError::Unauthorized);
            }
        }
        value.before_create(self);

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
                .map(|i| { format!("?{}", i.0 + 1) })
                .collect::<Vec<String>>()
                .join(", ")
        );
        debug!("querying database: {query}");

        let result = self.conn.execute(query, params_from_iter(value.params()));
        value.after_create(self);
        match result {
            Ok(result) => Ok(result),
            Err(err) => Err(DatabaseError::SqliteError(err)),
        }
    }

    pub fn update<T: DbObject>(
        &self,
        value: &T,
        user: Option<&User>,
    ) -> Result<usize, DatabaseError> {
        if let Some(user) = user {
            if !value.can_update(user) {
                return Err(DatabaseError::Unauthorized);
            }
        }
        value.before_update(self);

        let query = &format!(
            "UPDATE {} SET {} WHERE {} = {}{}",
            T::table_name(),
            T::columns()
                .iter()
                .enumerate()
                .map(|(id, column)| { format!("{} = ?{}", column.name, id + 1) })
                .collect::<Vec<String>>()
                .join(", "),
            T::columns()[T::id_column_index()].name,
            value.get_id().as_i64(),
            match user {
                Some(user) => {
                    format!(" AND {}", T::update_access().access_filter::<T>(user))
                }
                None => String::new(),
            }
        );
        debug!("querying database: {query}");

        let result = self.conn.execute(query, params_from_iter(value.params()));
        value.after_update(self);
        match result {
            Ok(result) => Ok(result),
            Err(err) => Err(DatabaseError::SqliteError(err)),
        }
    }

    pub fn remove<T: DbObject>(
        &self,
        value: &T,
        user: Option<&User>,
    ) -> Result<usize, DatabaseError> {
        if let Some(user) = user {
            if !value.can_update(user) {
                return Err(DatabaseError::Unauthorized);
            }
        }
        value.before_delete(self);

        let query = &format!(
            "DELETE FROM {} WHERE {} = ?1{}",
            T::table_name(),
            T::columns()[T::id_column_index()].name,
            match user {
                Some(user) => {
                    format!(" AND {}", T::update_access().access_filter::<T>(user))
                }
                None => String::new(),
            },
        );
        debug!("querying database: {query}");

        let result = self.conn.execute(query, params![value.get_id()]);
        value.after_delete(self);
        match result {
            Ok(result) => Ok(result),
            Err(err) => Err(DatabaseError::SqliteError(err)),
        }
    }

    pub fn get_one<T: DbObject>(&self, id: Id, user: Option<&User>) -> Result<T, DatabaseError> {
        let query = &format!(
            "SELECT * FROM {} WHERE {} = ?1{}",
            T::table_name(),
            T::columns()[T::id_column_index()].name,
            match user {
                Some(user) => {
                    format!(" AND {}", T::view_access().access_filter::<T>(user))
                }
                None => String::new(),
            }
        );

        debug!("querying database: {query}");
        match self
            .conn
            .query_row(query, params![id], |row| T::from_row(row))
        {
            Ok(result) => Ok(result),
            Err(err) => Err(DatabaseError::SqliteError(err)),
        }
    }

    pub fn list_all<T: DbObject>(&self, user: Option<&User>) -> Result<Vec<T>, DatabaseError> {
        self.list_filtered::<T>(vec![], user)
    }

    #[allow(clippy::needless_pass_by_value)]
    /// instead of returning [`rusqlite::Error::QueryReturnedNoRows`] it will return an empty vector
    pub fn list_filtered<T: DbObject>(
        &self,
        filters: Vec<(String, String)>,
        user: Option<&User>,
    ) -> Result<Vec<T>, DatabaseError> {
        let mut query = format!("SELECT * FROM {}", T::table_name());

        let (mut fields, params) = if filters.is_empty() {
            (vec![], vec![])
        } else {
            Self::construct_filters::<T>(&filters)
        };

        if let Some(user) = user {
            fields.push(
                T::view_access()
                    .access_filter::<T>(user)
                    .as_str()
                    .to_string(),
            );
        }

        if !fields.is_empty() {
            query += " WHERE ";
            query += fields.join(" AND ").as_str();
        }

        debug!("querying database: {query}");

        let mut stmt = self
            .conn
            .prepare(&query)
            .map_err(DatabaseError::SqliteError)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(params), T::from_row);

        match rows {
            Err(err) => match err {
                rusqlite::Error::QueryReturnedNoRows => Ok(vec![]),
                _ => Err(DatabaseError::SqliteError(err)),
            },
            Ok(rows) => Ok(rows
                .filter_map(|row| row.ok())
                .collect::<Vec<T>>()),
        }
    }

    //this code is absolute ass.
    fn construct_filters<T: DbObject>(
        filters: &Vec<(String, String)>,
    ) -> (Vec<String>, Vec<String>) {
        let mut new_filters = vec![];
        let mut values = vec![];
        for (field, value) in filters {
            if let Some(column) = T::get_column(field) {
                let mut value = value.clone();

                //look at this expression; study it even.
                let query = if let Some(value_stripped) = value.strip_prefix("!") {
                    value = value_stripped.to_string();
                    format!("NOT {}", column.name)
                } else {
                    column.name.clone()
                };

                match value.as_str() {
                    "null" => {} //stop from doing anything no null
                    "false" => value = "0".to_string(),
                    "true" => value = "1".to_string(),
                    _ => {
                        if column.data_type == Type::Id {
                            if let Ok(id) = Id::from_string(&value) {
                                value = id.as_i64().to_string()
                            }
                        }
                    }
                };

                if value == "null" {
                    new_filters.push(format!("{query} IS NULL"));
                } else {
                    values.push(value.clone());
                    new_filters.push(format!("{}=?{}", query, values.len()));
                }
            }
        }

        (new_filters, values)
    }

    /// This should only be used during testing or during first setup to create an admin account
    pub fn create_user(&self, username: &str, password: &str) -> anyhow::Result<User> {
        let user = User {
            username: username.to_string(),
            ..Default::default()
        };
        self.create_user_from(user, password)
    }
    /// This should only be used during testing or during first setup to create an admin account
    pub fn create_user_from(&self, user: User, password: &str) -> anyhow::Result<User> {
        self.insert(&user, None)?;

        self.insert(&Password::new(user.id, password), None)?;

        Ok(user)
    }
}

#[derive(Debug)]
pub enum DatabaseError {
    Unauthorized,
    NotFound,
    InternalServerError(String),
    SqliteError(rusqlite::Error),
}

impl Display for DatabaseError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            DatabaseError::Unauthorized => write!(f, "Unauthorized"),
            DatabaseError::NotFound => write!(f, "NotFound"),
            DatabaseError::InternalServerError(err) => write!(f, "Internal server error: {err}"),
            DatabaseError::SqliteError(err) => write!(f, "Sqlite error: {err}"),
        }
    }
}

impl Error for DatabaseError {}
impl Reject for DatabaseError {}

impl From<rusqlite::Error> for DatabaseError {
    fn from(error: rusqlite::Error) -> Self {
        DatabaseError::SqliteError(error)
    }
}

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
        memory_limit: None,
        world_limit: None,
        active_world_limit: None,
        storage_limit: None,
        is_privileged: false,
        enabled: false,
    };

    let mut user_max = User {
        id: Id::default(),
        username: "Username".to_string(),
        avatar_id: Some(Id::default()),
        memory_limit: Some(1024),
        world_limit: Some(10),
        active_world_limit: Some(3),
        storage_limit: Some(10240),
        is_privileged: true,
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
