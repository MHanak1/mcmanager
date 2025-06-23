use std::collections::HashMap;
use anyhow::Result;
use mcmanager::database::objects::{Group, User, World};
use mcmanager::database::types::Id;
use mcmanager::database::{Database, objects};
use mcmanager::util;
use std::path::Path;
use log::error;
use mcmanager::minecraft::server::ServerConfigLimit;

fn main() -> Result<()> {
    util::dirs::init_dirs().expect("Failed to initialize the data directory");
    let conn = rusqlite::Connection::open(Path::new(&util::dirs::data_dir().join("database.db")))?;
    let database = Database { conn };
    database.init().expect("Failed to init database");
    
    
    let first_launch = database.list_all::<User>(None)?.is_empty();
    
    if first_launch {
        util::dirs::init_dirs().expect("Failed to initialize the data directory");

        let default_group = {
            let mut config_limits =  HashMap::new();
            config_limits.insert(String::from("view-distance"), ServerConfigLimit::LessThan(12));
            config_limits.insert(String::from("simulation-distance"), ServerConfigLimit::LessThan(12));
            config_limits.insert(String::from("max-players"), ServerConfigLimit::LessThan(20));
            Group {
                id: Default::default(),
                name: "".to_string(),
                total_memory_limit: None,
                per_world_memory_limit: Some(2048),
                world_limit: Some(10),
                active_world_limit: Some(3),
                storage_limit: None,
                config_blacklist: vec![String::from("online-mode")],
                config_whitelist: vec![],
                config_limits,
                is_privileged: false,
            }
        };

        let admin_group = {
            Group {
                id: Default::default(),
                name: "".to_string(),
                total_memory_limit: None,
                per_world_memory_limit: None,
                world_limit: None,
                active_world_limit: None,
                storage_limit: None,
                config_blacklist: vec![],
                config_whitelist: vec![],
                config_limits: HashMap::new(),
                is_privileged: false,
            }
        };

        if let Err(err) = database.insert(&default_group, None) {error!("Failed to insert default user group: {}", err) }
        if let Err(err) = database.insert(&admin_group, None) {error!("Failed to insert admin group: {}", err) }

        println!(include_str!("resources/logo.txt"));
    }


    Ok(())
}
