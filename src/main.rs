use anyhow::{Result, bail};
use futures::{SinkExt, TryFutureExt};
use log::{error, info};
use mcmanager::api::serve::AppState;
use mcmanager::config::{CONFIG, DatabaseType};
use mcmanager::database::objects::{Group, ModLoader, User, World};
use mcmanager::database::types::Id;
use mcmanager::database::{Database, DatabasePool, objects};
use mcmanager::minecraft::server::{MinecraftServerCollection, ServerConfigLimit};
use mcmanager::minecraft::velocity::{InternalVelocityServer, VelocityServer};
use mcmanager::{bin, util};
use serde::Deserialize;
use sqlx::any::AnyPoolOptions;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{PgPool, SqliteConnection, SqlitePool};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use std::time::Duration;
use uuid::Uuid;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    util::dirs::init_dirs().expect("Failed to initialize the data directory");

    let secrets_path = util::dirs::base_dir().join("secrets.toml");
    if !secrets_path.exists() {
        let mut secrets_file = File::create(&secrets_path)?;
        secrets_file.write_all(format!("api_secret = {}\nforwarding_secret = {}", Uuid::new_v4().as_simple().to_string(), Uuid::new_v4().as_simple().to_string()).as_bytes())?;
        println!("secrets file written to {}", secrets_path.display());
    }

    let config_path = util::dirs::base_dir().join("config.toml");
    if !config_path.exists() {
        let mut config_file = File::create(&config_path)?;
        config_file.write_all(include_bytes!("resources/default_config.toml"))?;
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

        println!(include_str!("resources/logo.txt"));

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
                    id: Default::default(),
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
                        String::from(""),
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
                    id: Default::default(),
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
                        id: Default::default(),
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

            let mut velocity_config_file =
                File::create(&util::dirs::base_dir().join("velocity_config.toml"))
                    .expect("failed to create the velocity config file");
            velocity_config_file
                .write_all(include_bytes!("resources/velocity_config.toml"))
                .expect("failed to write default config file");

            // add some basic default values
            database
                .insert(
                    &ModLoader {
                        id: Default::default(),
                        name: "Vanilla".to_string(),
                        can_load_mods: false,
                    },
                    None,
                )
                .await?;
            database
                .insert(
                    &ModLoader {
                        id: Default::default(),
                        name: "Fabric".to_string(),
                        can_load_mods: true,
                    },
                    None,
                )
                .await?;
            database
                .insert(
                    &ModLoader {
                        id: Default::default(),
                        name: "Forge".to_string(),
                        can_load_mods: true,
                    },
                    None,
                )
                .await?;
        }

        {
            println!("would you like to download the latest version of velocity? [Y/n]");
            let mut input = String::new();
            std::io::stdin()
                .read_line(&mut input)
                .expect("Failed to read line");
            input = input.trim().to_ascii_lowercase();
            if input == "y" || input == "yes" || input.is_empty() {
                if let Err(err) = try_download_velocity().await {
                    error!("Could not download velocity: {}", err);
                } else {
                    println!("Successfully downloaded velocity");
                }
            }
        }
        println!("MCManager set up successfully. You can now restart this executable.");
        return Ok(());
    }

    let state = AppState {
        database,
        servers: MinecraftServerCollection::new(),
    };

    tokio::task::spawn({
        let servers = state.servers.clone();
        async move {
            let mut interval = tokio::time::interval(Duration::from_millis(1000));
            loop {
                interval.tick().await;
                servers.refresh_servers().await;
            }
        }
    });

    tokio::task::spawn({
        let servers = state.servers.clone();
        async move {
            info!("starting velocity at {}", CONFIG.velocity.port);
            let mut velocity_server =
                InternalVelocityServer::new(servers).expect("failed to create a velocity server");
            velocity_server
                .start()
                .await
                .expect("failed to start a velocity server");

            let mut interval = tokio::time::interval(Duration::from_millis(1000));
            loop {
                interval.tick().await;
                if let Err(err) = velocity_server.update().await {
                    error!("failed to update velocity server: {err}");
                }
            }
        }
    });

    mcmanager::api::serve::run(state, CONFIG.clone()).await?;
    Ok(())
}

async fn try_download_velocity() -> Result<()> {
    let client = reqwest::Client::new();
    #[derive(Debug, Deserialize)]
    struct Velocity {
        project_id: String,
        project_name: String,
        version_groups: Vec<String>,
        versions: Vec<String>,
    }

    let velocity: Velocity = serde_json::from_str(
        &client
            .get("https://api.papermc.io/v2/projects/velocity/")
            .send()
            .await?
            .text()
            .await?,
    )?;
    let version = velocity.versions.last().unwrap();

    info!("found the velocity version: {}", version);

    #[derive(Debug, Deserialize)]
    struct Builds {
        project_id: String,
        project_name: String,
        version: String,
        builds: Vec<Build>,
    }
    #[derive(Debug, Deserialize)]
    struct Build {
        build: i32,
        time: String,
        channel: String,
        promoted: bool,
        changes: Vec<Changes>,
        downloads: Downloads,
    }

    #[derive(Debug, Deserialize)]
    struct Changes {
        commit: String,
        summary: String,
        message: String,
    }

    #[derive(Debug, Deserialize)]
    struct Downloads {
        application: Application,
    }

    #[derive(Debug, Deserialize)]
    struct Application {
        name: String,
        sha256: String,
    }

    let velocity: Builds = serde_json::from_str(
        &client
            .get(
                format!(
                    "https://api.papermc.io/v2/projects/velocity/versions/{}/builds",
                    version
                )
                .as_str(),
            )
            .send()
            .await?
            .text()
            .await?,
    )?;
    let build = velocity.builds.last().unwrap();

    info!(
        "Downloading the latest version of velocity: {}",
        build.downloads.application.name
    );

    let response = reqwest::get(format!(
        "https://api.papermc.io/v2/projects/velocity/versions/{}/builds/{}/downloads/{}",
        version, build.build, build.downloads.application.name
    ))
    .await?;
    let dir = util::dirs::velocity_dir().join("velocity.jar");
    let mut dest = File::create(&dir)?;
    let content = response.bytes().await?;
    dest.write_all(&content)?;
    info!(
        "Successfully downloaded the latest version of velocity to {}",
        dir.display()
    );

    Ok(())
}
