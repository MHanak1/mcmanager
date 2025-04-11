use crate::database::objects::DbObject;
use crate::database::objects::{
    InviteLink, Mod, ModLoader, Password, Session, User, Version, World,
};
use crate::database::types::{Id, Type};
use argon2::password_hash::SaltString;
use argon2::password_hash::rand_core::OsRng;
use argon2::{Argon2, PasswordHasher};
use std::collections::HashMap;

pub mod objects;
pub mod types;

#[cfg(test)]
use crate::database::types::Token;

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

    pub fn insert(&self, value: &impl DbObject, user: Option<&User>) -> rusqlite::Result<usize> {
        value.insert_self(&self.conn, user)
    }

    pub fn update(&self, value: &impl DbObject, user: Option<&User>) -> rusqlite::Result<usize> {
        value.update_self(&self.conn, user)
    }

    pub fn remove(&self, value: &impl DbObject, user: Option<&User>) -> rusqlite::Result<usize> {
        value.remove_self(&self.conn, user)
    }

    pub fn get_one<T: DbObject>(&self, id: Id, user: Option<&User>) -> rusqlite::Result<T> {
        T::get_from_db(&self.conn, id, user)
    }

    pub fn list_all<T: DbObject>(&self, user: Option<&User>) -> rusqlite::Result<Vec<T>> {
        self.list_filtered::<T>(HashMap::default(), user)
    }

    pub fn list_filtered<T: DbObject>(
        &self,
        filters: HashMap<String, String>,
        user: Option<&User>,
    ) -> rusqlite::Result<Vec<T>> {
        let mut query = format!("SELECT * FROM {} WHERE ", T::table_name());

        let fields = if !filters.is_empty() {
            let (filters_processed, fields) = Self::construct_filters::<T>(&filters);

            for filter in filters_processed {
                query += filter.as_str();
                query += " AND ";
            }
            fields
        } else {
            vec![]
        };

        if let Some(user) = user {
            query += T::view_access().access_filter::<T>(user).as_str();
        }

        println!("{}", query);

        let mut stmt = self.conn.prepare(&query)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(fields), T::from_row)?;

        Ok(rows
            .filter_map(|row| match row {
                Ok(row) => Some(row),
                Err(_) => None,
            })
            .collect::<Vec<T>>())
    }

    //this code is absolute ass.
    fn construct_filters<T: DbObject>(
        filters: &HashMap<String, String>,
    ) -> (Vec<String>, Vec<String>) {
        let mut new_filters = vec![];
        let mut values = vec![];
        for (field, value) in filters {
            if let Some(column) = T::get_column(field) {
                let mut value = value.clone();

                //lawd isn't that a syntax
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
                    _ => if column.data_type == Type::Id { if let Ok(id) = Id::from_string(&value) { value = id.as_i64().to_string() } },
                };

                if value == "null" {
                    new_filters.push(format!("{} IS NULL", query));
                } else {
                    values.push(value.clone());
                    new_filters.push(format!("{}=?{}", query, values.len()));
                }
            }
        }

        (new_filters, values)
    }

    pub fn create_user(&self, username: String, password: &str) -> anyhow::Result<User> {
        let user = User {
            name: username,
            ..Default::default()
        };
        self.create_user_from(user, password)
    }
    pub fn create_user_from(&self, user: User, password: &str) -> anyhow::Result<User> {
        self.insert(&user, None)?;

        let salt = SaltString::generate(&mut OsRng);
        let argon = Argon2::default();

        self.insert(
            &Password {
                user_id: user.id,
                hash: argon
                    .hash_password(password.as_bytes(), &salt)
                    .unwrap()
                    .to_string(),
                salt,
            },
            None,
        )?;

        Ok(user)
    }
}

#[rustfmt::skip]
#[test]
pub fn manipulate_data() -> anyhow::Result<()> {
    use crate::database::types::Id;

    let conn = rusqlite::Connection::open_in_memory()?;

    let database = Database { conn };
    database.init()?;

    let mut mod_loader = ModLoader {
        id: Default::default(),
        name: "Mod Loader".to_string(),
        can_load_mods: false,
    };

    let mut version = Version {
        id: Default::default(),
        minecraft_version: "1.2.3.4".to_string(),
        mod_loader_id: mod_loader.id,
    };


    let mut user_min = User {
        id: Default::default(),
        name: String::new(),
        avatar_id: None,
        memory_limit: None,
        player_limit: None,
        world_limit: None,
        active_world_limit: None,
        storage_limit: None,
        is_privileged: false,
        enabled: false,
    };

    let mut user_max = User {
        id: Default::default(),
        name: "Username".to_string(),
        avatar_id: Some(Default::default()),
        memory_limit: Some(1024),
        player_limit: Some(10),
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
        id: Default::default(),
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
        icon_id: Some(Default::default()),
        owner_id: user_max.id,
    };

    let mut world_min = World {
        id: Default::default(),
        owner_id: user_min.id,
        name: String::new(),
        icon_id: None,
        allocated_memory: 0,
        version_id: version.id,
        enabled: false,
    };

    let mut world_max = World {
        id: Default::default(),
        owner_id: user_max.id,
        name: "World Name".to_string(),
        icon_id: Some(Default::default()),
        allocated_memory: 1024,
        version_id: version.id,
        enabled: true,
    };

    let mut session = Session {
        user_id: user_min.id,
        token: Default::default(),
        created: Default::default(),
        expires: false,
    };

    let mut invite_link = InviteLink {
        id: Default::default(),
        invite_token: Token::new(1),
        creator_id: user_min.id,
        created: Default::default(),
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

    println!("checking inserted objects");
    assert_eq!(mod_loader, ModLoader::get_from_db(&database.conn, mod_loader.id, None)?);
    assert_eq!(version, Version::get_from_db(&database.conn, version.id, None)?);
    assert_eq!(user_min, User::get_from_db(&database.conn, user_min.id, None)?);
    assert_eq!(user_max, User::get_from_db(&database.conn, user_max.id, None)?);
    assert_eq!(password, Password::get_from_db(&database.conn, password.user_id, None)?);
    assert_eq!(mc_mod_min, Mod::get_from_db(&database.conn, mc_mod_min.id, None)?);
    assert_eq!(mc_mod_max, Mod::get_from_db(&database.conn, Id::from_string("ABCDEFGH").unwrap(), None)?);
    assert_eq!(world_min, World::get_from_db(&database.conn, world_min.id, None)?);
    assert_eq!(world_max, World::get_from_db(&database.conn, world_max.id, None)?);
    assert_eq!(session, Session::get_from_db(&database.conn, session.user_id, None)?);
    assert_eq!(invite_link, InviteLink::get_from_db(&database.conn, invite_link.id, None)?);

    println!("altering values");
    mod_loader.name = "New Display Name".to_string();
    version.minecraft_version = "4.3.2.1".to_string();
    user_min.name = "New Username".to_string();
    user_max.name = "Other New Username".to_string();
    password.salt = SaltString::generate(OsRng);
    mc_mod_min.name = "New Mod Name".to_string();
    mc_mod_max.name = "New Mod Name".to_string();
    world_min.name = "New World Name".to_string();
    world_max.name = "New World Name".to_string();
    session.created = chrono::Utc::now(); //this test will fail on january 1st 1970 (highly unlikely)
    invite_link.invite_token = Token::new(1);

    println!("checking if objects no longer the same");
    assert_ne!(mod_loader, ModLoader::get_from_db(&database.conn, mod_loader.id, None)?);
    assert_ne!(version, Version::get_from_db(&database.conn, version.id, None)?);
    assert_ne!(user_min, User::get_from_db(&database.conn, user_min.id, None)?);
    assert_ne!(user_max, User::get_from_db(&database.conn, user_max.id, None)?);
    assert_ne!(password, Password::get_from_db(&database.conn, password.user_id, None)?);
    assert_ne!(mc_mod_min, Mod::get_from_db(&database.conn, mc_mod_min.id, None)?);
    assert_ne!(mc_mod_max, Mod::get_from_db(&database.conn, mc_mod_max.id, None)?);
    assert_ne!(world_min, World::get_from_db(&database.conn, world_min.id, None)?);
    assert_ne!(world_max, World::get_from_db(&database.conn, world_max.id, None)?);
    assert_ne!(session, Session::get_from_db(&database.conn, session.user_id, None)?);
    assert_ne!(invite_link, InviteLink::get_from_db(&database.conn, invite_link.id, None)?);

    println!("updating objects");
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

    println!("checking if objects are again the same");
    assert_eq!(mod_loader, ModLoader::get_from_db(&database.conn, mod_loader.id, None)?);
    assert_eq!(version, Version::get_from_db(&database.conn, version.id, None)?);
    assert_eq!(user_min, User::get_from_db(&database.conn, user_min.id, None)?);
    assert_eq!(user_max, User::get_from_db(&database.conn, user_max.id, None)?);
    assert_eq!(password, Password::get_from_db(&database.conn, password.user_id, None)?);
    assert_eq!(mc_mod_min, Mod::get_from_db(&database.conn, mc_mod_min.id, None)?);
    assert_eq!(mc_mod_max, Mod::get_from_db(&database.conn, mc_mod_max.id, None)?);
    assert_eq!(world_min, World::get_from_db(&database.conn, world_min.id, None)?);
    assert_eq!(world_max, World::get_from_db(&database.conn, world_max.id, None)?);
    assert_eq!(session, Session::get_from_db(&database.conn, session.user_id, None)?);
    assert_eq!(invite_link, InviteLink::get_from_db(&database.conn, invite_link.id, None)?);

    println!("removing objects");
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

    println!("checking if objects are actually removed");
    assert_eq!(Err(rusqlite::Error::QueryReturnedNoRows), ModLoader::get_from_db(&database.conn, mod_loader.id, None));
    assert_eq!(Err(rusqlite::Error::QueryReturnedNoRows), Version::get_from_db(&database.conn, version.id, None));
    assert_eq!(Err(rusqlite::Error::QueryReturnedNoRows), User::get_from_db(&database.conn, user_min.id, None));
    assert_eq!(Err(rusqlite::Error::QueryReturnedNoRows), User::get_from_db(&database.conn, user_max.id, None));
    assert_eq!(Err(rusqlite::Error::QueryReturnedNoRows), Password::get_from_db(&database.conn, password.user_id, None));
    assert_eq!(Err(rusqlite::Error::QueryReturnedNoRows), Mod::get_from_db(&database.conn, mc_mod_min.id, None));
    assert_eq!(Err(rusqlite::Error::QueryReturnedNoRows), Mod::get_from_db(&database.conn, mc_mod_max.id, None));
    assert_eq!(Err(rusqlite::Error::QueryReturnedNoRows), World::get_from_db(&database.conn, world_min.id, None));
    assert_eq!(Err(rusqlite::Error::QueryReturnedNoRows), World::get_from_db(&database.conn, world_max.id, None));
    assert_eq!(Err(rusqlite::Error::QueryReturnedNoRows), Session::get_from_db(&database.conn, session.user_id, None));
    assert_eq!(Err(rusqlite::Error::QueryReturnedNoRows), InviteLink::get_from_db(&database.conn, invite_link.id, None));

    Ok(())
}
