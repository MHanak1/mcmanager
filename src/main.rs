use color_eyre::Result;
use log::{error, info};
use mcmanager::api::serve::AppState;
use mcmanager::config::{CONFIG, DatabaseType};
use mcmanager::database::objects::{Group, ModLoader, User};
use mcmanager::database::{Database, DatabasePool};
use mcmanager::minecraft::proxy::{InfrarustServer, MinecraftProxy};
use mcmanager::minecraft::server::{MinecraftServerCollection, ServerConfigLimit};
use mcmanager::util;
use sqlx::postgres::PgPoolOptions;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};
use std::time::Duration;
use uuid::Uuid;
use mcmanager::database::types::Id;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    util::dirs::init_dirs().expect("Failed to initialize the data directory");

    let secrets_path = util::dirs::base_dir().join("secrets.toml");
    if !secrets_path.exists() {
        let mut secrets_file = File::create(&secrets_path)?;
        secrets_file.write_all(
            format!(
                "api_secret = \"{}\"\nforwarding_secret = \"{}\"",
                Uuid::new_v4().as_simple(),
                Uuid::new_v4().as_simple()
            )
            .as_bytes(),
        )?;
        println!("secrets file written to {}", secrets_path.display());
    }

    let config_path = util::dirs::base_dir().join("config.toml");
    if !config_path.exists() {
        let mut config_file = File::create(&config_path)?;
        config_file.write_all(include_bytes!("resources/configs/default_config.toml"))?;
        println!("Config file written to {}", config_path.display());
        println!("You can now edit the values in the config file and restart this executable.");
        return Ok(());
    }

    let pool: DatabasePool = match CONFIG.database.database_type {
        DatabaseType::Sqlite => {
            let options = SqlitePoolOptions::new().max_connections(CONFIG.database.max_connections);

            options
                .connect_with(
                    SqliteConnectOptions::new()
                        .filename(util::dirs::data_dir().join("database.db"))
                        .create_if_missing(true),
                )
                .await?
                .into()
        }
        DatabaseType::Postgres => PgPoolOptions::new()
            .max_connections(CONFIG.database.max_connections)
            .connect(CONFIG.database.pg_host.as_str())
            .await?
            .into(),
    };

    /*
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect("postgres://postgres:password@localhost")
        .await?;
     */

    let database = Database::new(pool);
    database.init().await.expect("Failed to init database");

    let second_launch = database.get_all::<User>(None).await?.is_empty();

    if second_launch {
        util::dirs::init_dirs().expect("Failed to initialize the data directory");

        println!(include_str!("resources/icons/logo.txt"));

        {
            print!("Enter the username for the administrator account.\nUsername: ");
            std::io::stdout().flush().expect("Failed to flush stdout");
            let mut username = String::new();
            std::io::stdin()
                .read_line(&mut username)
                .expect("Failed to read STDIN");

            print!("Enter the password for the administrator account.\nPassword: ");
            std::io::stdout().flush().expect("Failed to flush stdout");
            let mut password = String::new();
            std::io::stdin()
                .read_line(&mut password)
                .expect("Failed to read STDIN");
            print!("Confirm password: ");
            std::io::stdout().flush().expect("Failed to flush stdout");
            let mut confirm_password = String::new();
            std::io::stdin()
                .read_line(&mut confirm_password)
                .expect("Failed to read STDIN");

            if password != confirm_password {
                println!("Passwords don't match");
                return Ok(());
            }

            let default_group = {
                let mut config_limits = HashMap::new();
                config_limits.insert(
                    String::from("view-distance"),
                    ServerConfigLimit::LessThan(12),
                );
                config_limits.insert(
                    String::from("simulation-distance"),
                    ServerConfigLimit::LessThan(12),
                );
                config_limits.insert(String::from("max-players"), ServerConfigLimit::LessThan(20));
                Group {
                    id: Id::default(),
                    name: "User".to_string(),
                    total_memory_limit: None,
                    per_world_memory_limit: Some(2048),
                    world_limit: Some(10),
                    active_world_limit: Some(3),
                    storage_limit: None,
                    config_blacklist: vec![
                        String::from("online-mode"),
                        String::from("server-port"),
                    ],
                    config_whitelist: vec![
                        String::new(),
                        String::from("allow-flight"),
                        String::from("allow-nether"),
                        String::from("broadcast-console-to-ops"),
                        String::from("difficulty"),
                        String::from("enable-command-block"),
                        String::from("enable-status"),
                        String::from("enforce-secure-profile"),
                        String::from("enforce-whitelist"),
                        String::from("entity-broadcast-range-percentage"),
                        String::from("force-gamemode"),
                        String::from("function-permission-level"),
                        String::from("gamemode"),
                        String::from("generate-structures"),
                        String::from("generator-settings"),
                        String::from("hardcore"),
                        String::from("hide-online-players"),
                        String::from("initial-disabled-packs"),
                        String::from("initial-enabled-packs"),
                        String::from("level-seed"),
                        String::from("level-type"),
                        String::from("log-ips"),
                        String::from("max-chained-neighbor-updates"),
                        String::from("max-players"),
                        String::from("max-tick-time"),
                        String::from("max-world-size"),
                        String::from("motd"),
                        String::from("op-permission-level"),
                        String::from("player-idle-timeout"),
                        String::from("player-idle-timeout"),
                        String::from("pvp"),
                        String::from("require-resource-pack"),
                        String::from("resource-pack"),
                        String::from("resource-pack-id"),
                        String::from("resource-pack-prompt"),
                        String::from("resource-pack-sha1"),
                        String::from("simulation-distance"),
                        String::from("spawn-monsters"),
                        String::from("spawn-protection"),
                        String::from("view-distance"),
                        String::from("white-list"),
                    ],
                    config_limits,
                    can_upload_mods: false,
                    is_privileged: false,
                }
            };

            let admin_group = {
                Group {
                    id: Id::default(),
                    name: "Admin".to_string(),
                    total_memory_limit: None,
                    per_world_memory_limit: None,
                    world_limit: None,
                    active_world_limit: None,
                    storage_limit: None,
                    config_blacklist: vec![],
                    config_whitelist: vec![],
                    config_limits: HashMap::new(),
                    can_upload_mods: true,
                    is_privileged: true,
                }
            };

            database
                .insert(&default_group, None)
                .await
                .expect("Failed to insert default user group");
            database
                .insert(&admin_group, None)
                .await
                .expect("Failed to insert administrator group");

            database
                .create_user_from(
                    User {
                        id: Id::default(),
                        username: String::from(username.trim()),
                        group_id: admin_group.id,
                        total_memory_usage: 0,
                        enabled: true,
                    },
                    password.trim(),
                )
                .await?;

            let mut config_file = File::open(&config_path)?;
            let mut config_contents = String::new();
            config_file.read_to_string(&mut config_contents)?;
            drop(config_file);

            let mut config_file = File::create(&config_path)?;
            config_file
                .write_all(
                    config_contents
                        .replace("AAAAAAAA", default_group.id.to_string().as_str())
                        .as_bytes(),
                )
                .expect("failed to write to the config file");

            // add some basic default values
            database
                .insert(
                    &ModLoader {
                        id: Id::default(),
                        name: "Vanilla".to_string(),
                        can_load_mods: false,
                    },
                    None,
                )
                .await?;
            database
                .insert(
                    &ModLoader {
                        id: Id::default(),
                        name: "Fabric".to_string(),
                        can_load_mods: true,
                    },
                    None,
                )
                .await?;
            database
                .insert(
                    &ModLoader {
                        id: Id::default(),
                        name: "Forge".to_string(),
                        can_load_mods: true,
                    },
                    None,
                )
                .await?;
        }

        println!("MCManager set up successfully. You can now restart this executable.");
        return Ok(());
    }

    let console_tickets = moka::future::CacheBuilder::new(10000) //10000 ought to be enough
        .time_to_live(Duration::from_secs(30*60)) // 30 minute ttl ought to be enough
        .build();

    let state = AppState {
        database,
        servers: MinecraftServerCollection::new(),
        console_tickets
    };

    tokio::task::spawn({
        let servers = state.servers.clone();
        async move {
            let mut interval = tokio::time::interval(Duration::from_millis(1000));
            loop {
                interval.tick().await;
                servers.poll_servers().await;
            }
        }
    });

    tokio::task::spawn({
        let servers = state.servers.clone();
        async move {
            info!("starting minecraft proxy at {}", CONFIG.proxy.port);
            let mut proxy =
                InfrarustServer::new(servers).expect("failed to create an infrarust server");
            proxy
                .start()
                .await
                .expect("failed to start an infrarust server");

            let mut interval = tokio::time::interval(Duration::from_millis(1000));
            loop {
                interval.tick().await;
                if let Err(err) = proxy.update().await {
                    error!("failed to update the infrarust server: {err}");
                }
            }
        }
    });

    mcmanager::api::serve::run(state, CONFIG.clone()).await?;
    Ok(())
}