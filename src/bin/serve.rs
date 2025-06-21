use log::{error, info};
use mcmanager::api::filters;
use mcmanager::api::handlers::ApiObject;
use mcmanager::config;
use mcmanager::config::CONFIG;
use mcmanager::database::Database;
use mcmanager::database::objects::{InviteLink, Mod, ModLoader, Session, User, Version, World};
use mcmanager::minecraft::velocity::{InternalVelocityServer, VelocityServer};
use mcmanager::{api, util};
use std::io::Write;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use test_log::test;
use tokio::sync::Mutex;
use warp::Filter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tokio::task::spawn(async {
        let mut interval = tokio::time::interval(Duration::from_millis(1000));
        loop {
            interval.tick().await;
            mcmanager::minecraft::server::util::refresh_servers().await;
        }
    });

    tokio::task::spawn(async {
        info!("starting velocity at {}", CONFIG.velocity.port);
        let mut velocity_server =
            InternalVelocityServer::new().expect("failed to create a velocity server");
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
    });

    env_logger::init();
    let conn = rusqlite::Connection::open(Path::new(&util::dirs::data_dir().join("database.db")))
        .expect("failed to open the database");
    let database = Database { conn };
    database.init().expect("failed to initialize database");
    let config_path = util::dirs::base_dir().join("config.toml");
    if !config_path.exists() {
        let mut config_file = std::fs::File::create(config_path)?;
        config_file.write_all(include_bytes!("../resources/default_config.toml"))?;
    }

    run(database, config::CONFIG.clone()).await;
    Ok(())
}

pub async fn run(database: Database, config: config::Config) {
    util::dirs::init_dirs().expect("Failed to initialize the data directory");

    let db_mutex = Arc::new(Mutex::new(database));

    let limit = config.public_routes_rate_limit;

    let public_routes_rate_limit =
        warp_rate_limit::RateLimitConfig::max_per_window(limit.0, limit.1);

    let limit = config.private_routes_rate_limit;
    let private_routes_rate_limit =
        warp_rate_limit::RateLimitConfig::max_per_window(limit.0, limit.1);

    let login = warp::post()
        .and(warp_rate_limit::with_rate_limit(public_routes_rate_limit))
        .and(warp::path!("api" / "login"))
        .and(warp::path::end())
        .and(filters::with_db(db_mutex.clone()))
        .and(warp::body::content_length_limit(1024 * 16))
        .and(warp::body::json())
        .and_then(api::handlers::user_auth);

    let user_info = warp::get()
        .and(warp::path!("api" / "user"))
        .and(warp::path::end())
        .and(filters::with_auth(db_mutex.clone()))
        .and_then(api::handlers::user_info);

    /*
    let mods = Mod::list_filter(db_mutex.clone(), private_routes_rate_limit.clone())
        .or(Mod::create_filter(
            db_mutex.clone(),
            private_routes_rate_limit.clone(),
        ))
        .or(Mod::update_filter(
            db_mutex.clone(),
            private_routes_rate_limit.clone(),
        ))
        .or(Mod::get_filter(
            db_mutex.clone(),
            private_routes_rate_limit.clone(),
        ))
        .or(Mod::remove_filter(
            db_mutex.clone(),
            private_routes_rate_limit.clone(),
        ));
    let versions = Version::list_filter(db_mutex.clone(), private_routes_rate_limit.clone())
        .or(Version::create_filter(
            db_mutex.clone(),
            private_routes_rate_limit.clone(),
        ))
        .or(Version::update_filter(
            db_mutex.clone(),
            private_routes_rate_limit.clone(),
        ))
        .or(Version::get_filter(
            db_mutex.clone(),
            private_routes_rate_limit.clone(),
        ))
        .or(Version::remove_filter(
            db_mutex.clone(),
            private_routes_rate_limit.clone(),
        ));
    let mod_loaders = ModLoader::list_filter(db_mutex.clone(), private_routes_rate_limit.clone())
        .or(ModLoader::create_filter(
            db_mutex.clone(),
            private_routes_rate_limit.clone(),
        ))
        .or(ModLoader::update_filter(
            db_mutex.clone(),
            private_routes_rate_limit.clone(),
        ))
        .or(ModLoader::get_filter(
            db_mutex.clone(),
            private_routes_rate_limit.clone(),
        ))
        .or(ModLoader::remove_filter(
            db_mutex.clone(),
            private_routes_rate_limit.clone(),
        ));
    let worlds = World::list_filter(db_mutex.clone(), private_routes_rate_limit.clone())
        .or(World::status_filter (
            db_mutex.clone(),
            private_routes_rate_limit.clone(),
        ))
        .or(World::create_filter(
            db_mutex.clone(),
            private_routes_rate_limit.clone(),
        ))
        .or(World::update_filter(
            db_mutex.clone(),
            private_routes_rate_limit.clone(),
        ))
        .or(World::get_filter(
            db_mutex.clone(),
            private_routes_rate_limit.clone(),
        ))
        .or(World::remove_filter(
            db_mutex.clone(),
            private_routes_rate_limit.clone(),
        ));
    let users = User::list_filter(db_mutex.clone(), private_routes_rate_limit.clone())
        .or(User::create_filter(
            db_mutex.clone(),
            private_routes_rate_limit.clone(),
        ))
        .or(User::update_filter(
            db_mutex.clone(),
            private_routes_rate_limit.clone(),
        ))
        .or(User::get_filter(
            db_mutex.clone(),
            private_routes_rate_limit.clone(),
        ))
        .or(User::remove_filter(
            db_mutex.clone(),
            private_routes_rate_limit.clone(),
        ));
    let sessions = Session::list_filter(db_mutex.clone(), private_routes_rate_limit.clone())
        .or(Session::create_filter(
            db_mutex.clone(),
            private_routes_rate_limit.clone(),
        ))
        .or(Session::get_filter(
            db_mutex.clone(),
            private_routes_rate_limit.clone(),
        ))
        .or(Session::remove_filter(
            db_mutex.clone(),
            private_routes_rate_limit.clone(),
        ));
    let invite_links = InviteLink::list_filter(db_mutex.clone(), private_routes_rate_limit.clone())
        .or(InviteLink::create_filter(
            db_mutex.clone(),
            private_routes_rate_limit.clone(),
        ))
        .or(InviteLink::get_filter(
            db_mutex.clone(),
            private_routes_rate_limit.clone(),
        ))
        .or(InviteLink::remove_filter(
            db_mutex.clone(),
            private_routes_rate_limit.clone(),
        ));
     */

    let log = warp::log("info");

    warp::serve(
        login
            .or(user_info)
            .or(Mod::filters(
                db_mutex.clone(),
                private_routes_rate_limit.clone(),
            ))
            .or(Version::filters(
                db_mutex.clone(),
                private_routes_rate_limit.clone(),
            ))
            .or(ModLoader::filters(
                db_mutex.clone(),
                private_routes_rate_limit.clone(),
            ))
            .or(World::filters(
                db_mutex.clone(),
                private_routes_rate_limit.clone(),
            ))
            .or(User::filters(
                db_mutex.clone(),
                private_routes_rate_limit.clone(),
            ))
            .or(Session::filters(
                db_mutex.clone(),
                private_routes_rate_limit.clone(),
            ))
            .or(InviteLink::filters(
                db_mutex.clone(),
                private_routes_rate_limit.clone(),
            ))
            .recover(api::handlers::handle_rejection)
            .with(log),
    )
    .run(SocketAddr::new(
        IpAddr::V4(Ipv4Addr::from_str(&config.listen_address).expect("invalid listen_address")),
        config.listen_port as u16,
    ))
    .await;
}

#[test]
#[allow(unused)]
fn user_creation_and_removal() -> anyhow::Result<()> {
    const TEST_PORT: u32 = 3031;

    use log::info;
    use mcmanager::config;
    use mcmanager::config::CONFIG;
    use mcmanager::database::types::Id;
    use pretty_assertions::assert_eq;
    use reqwest::header;
    use serde::Deserialize;
    use serde_with::serde_derive::Serialize;
    use std::thread;
    use warp::body::BodyDeserializeError;

    let conn = rusqlite::Connection::open_in_memory().expect("Can't open database connection");

    let database = Database { conn };
    database.init().expect("Can't init database");
    let mut user = database.create_user("Admin", "Password0")?;
    user.is_privileged = true;
    database.update(&user, None)?;

    thread::spawn(|| {
        let mut config = CONFIG.clone();
        config.listen_port = TEST_PORT;
        // essentially disables the rate limit
        config.public_routes_rate_limit = (10000000, 1);
        config.private_routes_rate_limit = (1000000, 1);
        let rt = tokio::runtime::Runtime::new().expect("Can't create runtime");
        rt.block_on(run(database, config))
    });

    //wait for the server to start
    thread::sleep(std::time::Duration::from_secs(1));

    let url = format!("http://{}:{}/api", CONFIG.listen_address, TEST_PORT);

    let client = reqwest::blocking::Client::new();

    #[derive(Deserialize)]
    struct TokenReply {
        token: String,
    }

    info!("logging in as admin");
    let token: TokenReply = serde_json::from_str(
        &client
            .post(format!("{url}/login"))
            .body("{\"username\": \"Admin\", \"password\": \"Password0\"}")
            .send()?
            .text()?,
    )?;
    let token = token.token;

    assert_eq!(
        user,
        serde_json::from_str(
            &client
                .get(format!("{url}/user"))
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .send()?
                .text()?,
        )?
    );

    info!("Creating User1 with minimal fields");
    let created: User = serde_json::from_str(
        &client
            .post(format!("{url}/users"))
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body("{\"username\": \"User1\", \"password\": \"Password1\"}")
            .send()?
            .text()?,
    )?;

    let mut usera = User::default();
    usera.id = created.id;
    usera.username = "User1".to_string();

    assert_eq!(created, usera);

    let _: TokenReply = serde_json::from_str(
        &client
            .post(format!("{url}/login"))
            .body("{\"username\": \"User1\", \"password\": \"Password1\"}")
            .send()?
            .text()?,
    )?;

    #[derive(Serialize)]
    struct UserSend {
        username: String,
        password: String,
        avatar_id: Option<Id>,
        memory_limit: Option<u32>,
        world_limit: Option<u32>,
        active_world_limit: Option<u32>,
        storage_limit: Option<u32>,
        is_privileged: bool,
        enabled: bool,
    }

    let mut userb = User {
        id: Default::default(),
        username: "User2".to_string(),
        avatar_id: None,
        memory_limit: None,
        world_limit: None,
        active_world_limit: None,
        storage_limit: None,
        is_privileged: false,
        enabled: true,
    };

    info!("Creating User2 with all possible field set to null");
    let created: User = serde_json::from_str(
        &client
            .post(format!("{url}/users"))
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body(
                serde_json::to_string(&UserSend {
                    username: userb.username.clone(),
                    password: "Password2".to_string(),
                    avatar_id: userb.avatar_id,
                    memory_limit: userb.memory_limit,
                    world_limit: userb.world_limit,
                    active_world_limit: userb.active_world_limit,
                    storage_limit: userb.storage_limit,
                    is_privileged: userb.is_privileged,
                    enabled: userb.enabled,
                })
                .unwrap(),
            )
            .send()?
            .text()?,
    )?;

    userb.id = created.id;

    assert_eq!(created, userb);
    println!(
        "{:?}",
        &client
            .post(format!("{url}/login"))
            .body("{\"username\": \"User2\", \"password\": \"Password2\"}")
            .send()?
            .text()?
    );

    let _: TokenReply = serde_json::from_str(
        &client
            .post(format!("{url}/login"))
            .body("{\"username\": \"User2\", \"password\": \"Password2\"}")
            .send()?
            .text()?,
    )?;

    let tmp_id = Id::new_random();
    let mut usera = User {
        id: Default::default(),
        username: "User3".to_string(),
        avatar_id: Some(tmp_id),
        memory_limit: Some(1234),
        world_limit: Some(1234),
        active_world_limit: Some(1234),
        storage_limit: Some(1234),
        is_privileged: true,
        enabled: true,
    };

    info!("Creating User3 with all possible field set");
    let userb: User = serde_json::from_str(
        &client
            .post(format!("{url}/users"))
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body(
                serde_json::to_string(&UserSend {
                    username: usera.username.clone(),
                    password: "Password3".to_string(),
                    avatar_id: usera.avatar_id,
                    memory_limit: usera.memory_limit,
                    world_limit: usera.world_limit,
                    active_world_limit: usera.active_world_limit,
                    storage_limit: usera.storage_limit,
                    is_privileged: usera.is_privileged,
                    enabled: usera.enabled,
                })
                .unwrap(),
            )
            .send()?
            .text()?,
    )?;

    usera.id = userb.id;

    assert_eq!(userb, usera);

    let user_token: TokenReply = serde_json::from_str(
        &client
            .post(format!("{url}/login"))
            .body("{\"username\": \"User3\", \"password\": \"Password3\"}")
            .send()?
            .text()?,
    )?;
    let user_token = user_token.token;

    info!("Creating disabled User4");
    let created: User = serde_json::from_str(
        &client
            .post(format!("{url}/users"))
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body("{\"username\": \"User4\", \"password\": \"Password4\", \"enabled\": false}")
            .send()?
            .text()?,
    )?;

    let reply: Result<TokenReply, serde_json::Error> = serde_json::from_str(
        &client
            .post(format!("{url}/login"))
            .body("{\"username\": \"User4\", \"password\": \"Password4\"}")
            .send()?
            .text()?,
    );

    assert!(reply.is_err());

    info!("enabling User5");
    let created: User = serde_json::from_str(
        &client
            .put(format!("{url}/users/{}", created.id))
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body("{\"enabled\": true}")
            .send()?
            .text()?,
    )?;

    println!("{:?}", created);

    let reply: Result<TokenReply, serde_json::Error> = serde_json::from_str(
        &client
            .post(format!("{url}/login"))
            .body("{\"username\": \"User4\", \"password\": \"Password4\"}")
            .send()?
            .text()?,
    );

    assert!(reply.is_ok());

    info!("updating User5 username");
    let created: User = serde_json::from_str(
        &client
            .put(format!("{url}/users/{}", created.id))
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body("{\"username\": \"User4New\"}")
            .send()?
            .text()?,
    )?;
    let reply: Result<TokenReply, serde_json::Error> = serde_json::from_str(
        &client
            .post(format!("{url}/login"))
            .body("{\"username\": \"User4New\", \"password\": \"Password4\"}")
            .send()?
            .text()?,
    );
    assert!(reply.is_ok());

    info!("updating User5 password");
    let created: User = serde_json::from_str(
        &client
            .put(format!("{url}/users/{}", created.id))
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body("{\"password\": \"Password4New\"}")
            .send()?
            .text()?,
    )?;
    let reply: Result<TokenReply, serde_json::Error> = serde_json::from_str(
        &client
            .post(format!("{url}/login"))
            .body("{\"username\": \"User4New\", \"password\": \"Password4New\"}")
            .send()?
            .text()?,
    );
    assert!(reply.is_ok());

    Ok(())
}

/*
#[test]
#[allow(unused)]
fn permissions() -> anyhow::Result<()> {
    const TEST_PORT: u32 = 3032;

    use mcmanager::config;
    use mcmanager::config::CONFIG;
    use pretty_assertions::assert_eq;
    use reqwest::header;
    use serde::Deserialize;
    use std::thread;

    let conn = rusqlite::Connection::open_in_memory().expect("Can't open database connection");

    let database = Database { conn };
    database.init().expect("Can't init database");
    let mut admin = database.create_user("Admin", "Password1")?;
    admin.is_privileged = true;
    database.update(&admin, None)?;

    thread::spawn(|| {
        let mut config = CONFIG.clone();
        config.listen_port = TEST_PORT;
        let rt = tokio::runtime::Runtime::new().expect("Can't create runtime");
        rt.block_on(run(database, config))
    });

    //wait for the server to start
    thread::sleep(std::time::Duration::from_secs(1));

    let url = format!("http://{}:{}/api", CONFIG.listen_address, TEST_PORT);

    let client = reqwest::blocking::Client::new();

    #[derive(Deserialize)]
    struct TokenReply {
        token: String,
    }

    let admin_token: TokenReply = serde_json::from_str(
        &client
            .post(format!("{url}/login"))
            .body("{\"username\": \"Admin\", \"password\": \"Password1\"}")
            .send()?
            .text()?,
    )?;
    let admin_token = admin_token.token;

    let got_user: User = serde_json::from_str(
        &client
            .get(format!("{url}/user"))
            .header(header::AUTHORIZATION, format!("Bearer {admin_token}"))
            .send()?
            .text()?,
    )?;

    assert_eq!(admin, got_user);

    let user1: User = serde_json::from_str(
        &client
            .post(format!("{url}/users"))
            .header(header::AUTHORIZATION, format!("Bearer {admin_token}"))
            .body("{\"username\": \"User1\", \"password\": \"Password2\"}")
            .send()?
            .text()?,
    )?;

    let user2: User = serde_json::from_str(
        &client
            .post(format!("{url}/users"))
            .header(header::AUTHORIZATION, format!("Bearer {admin_token}"))
            .body("{\"username\": \"User2\", \"password\": \"Password3\"}")
            .send()?
            .text()?,
    )?;

    let user1_token = client
        .post(format!("{url}/login"))
        .body("{\"username\": \"User1\", \"password\": \"Password2\"}")
        .send()?
        .text()?
        .replace("\"", "");

    let user1_token: TokenReply = serde_json::from_str(
        &client
            .post(format!("{url}/login"))
            .body("{\"username\": \"User1\", \"password\": \"Password2\"}")
            .send()?
            .text()?,
    )?;
    let user1_token = user1_token.token;

    let user2_token: TokenReply = serde_json::from_str(
        &client
            .post(format!("{url}/login"))
            .body("{\"username\": \"User2\", \"password\": \"Password3\"}")
            .send()?
            .text()?,
    )?;
    let user2_token = user2_token.token;

    let admin_user_access: Vec<User> = serde_json::from_str(
        &client
            .get(format!("{url}/users"))
            .header(header::AUTHORIZATION, format!("Bearer {admin_token}"))
            .send()?
            .text()?,
    )?;

    let normal_user_access: Vec<User> = serde_json::from_str(
        &client
            .get(format!("{url}/users"))
            .header(header::AUTHORIZATION, format!("Bearer {user1_token}"))
            .send()?
            .text()?,
    )?;

    assert_eq!(
        admin_user_access,
        vec![admin.clone(), user1.clone(), user2.clone()]
    );
    assert_eq!(normal_user_access, vec![user1.clone()]);

    Ok(())
}
 */
