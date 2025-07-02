use crate::api::filters;
use crate::api::handlers::ApiObject;
use crate::config;
use crate::config::CONFIG;
use crate::database::objects::{Group, InviteLink, Mod, ModLoader, Session, User, Version, World};
use crate::database::{Database};
use crate::minecraft::velocity::{InternalVelocityServer, VelocityServer};
use crate::{api, util};
use log::{error, info};
use sqlx::Encode;
use sqlx::any::AnyPoolOptions;
use sqlx::sqlite::SqlitePoolOptions;
use std::io::Write;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use axum::extract::{MatchedPath, State};
use axum::http::Request;
use axum::Router;
use axum::routing::{get, post, MethodRouter};
use static_dir::static_dir;
use test_log::test;
use tokio::sync::Mutex;
use tracing::{info_span};

pub type AppState = Database;

pub async fn run(database: Database, config: config::Config) -> Result<(), anyhow::Error> {
    util::dirs::init_dirs().expect("Failed to initialize the data directory");

    let limit = config.public_routes_rate_limit;

    //TODO: Re-add Rate Limits

    let limit = config.private_routes_rate_limit;
    /*

    let register = axum::post()
        .and(axum_rate_limit::with_rate_limit(
            public_routes_rate_limit.clone(),
        ))
        .and(axum::path!("api" / "register"))
        .and(filters::with_db(database.clone()))
        .and(axum::body::content_length_limit(1024 * 16))
        .and(axum::body::json())
        .and(axum::query::<Vec<(String, String)>>())
        .and_then(api::handlers::user_register);

    let login = axum::post()
        .and(axum_rate_limit::with_rate_limit(
            public_routes_rate_limit.clone(),
        ))
        .and(axum::path!("api" / "login"))
        .and(filters::with_db(database.clone()))
        .and(axum::body::content_length_limit(1024 * 16))
        .and(axum::body::json())
        .and_then(api::handlers::user_auth);

    let logout = axum::post()
        .and(axum_rate_limit::with_rate_limit(
            private_routes_rate_limit.clone(),
        ))
        .and(axum::path!("api" / "logout"))
        .and(filters::with_db(database.clone()))
        .and(filters::with_bearer_token())
        .and_then(api::handlers::logout);

    let user_info = axum::get()
        .and(axum_rate_limit::with_rate_limit(
            private_routes_rate_limit.clone(),
        ))
        .and(axum::path!("api" / "user"))
        .and(filters::with_auth(database.clone()))
        .and_then(api::handlers::user_info);

    let server_info = axum::get()
        .and(axum_rate_limit::with_rate_limit(
            public_routes_rate_limit.clone(),
        ))
        .and(axum::path!("api" / "info"))
        .and_then(api::handlers::server_info);

    let is_taken = axum::get()
        .and(axum_rate_limit::with_rate_limit(
            private_routes_rate_limit.clone(),
        ))
        .and(axum::path!("api" / "valid" / String / String))
        .and(axum::get())
        .and(filters::with_db(database.clone()))
        .and_then(api::handlers::check_free);

    // a hacky way to serve the frontend.
    let frontend = static_dir!("mcmanager-frontend/dist")
        .or(axum::path::full().and_then(move |path: FullPath| async move {
            let path = path.as_str();
            if path.starts_with("/api") {
                return Err(axum::reject());
            }

            if path.contains(".") {
                return Err(axum::reject());
            }

            Ok(axum::response::html(include_str!("../../mcmanager-frontend/dist/index.html")))
        }));

    let log = axum::log("info");
     */

    let api = Router::new()
        .route("/login", post(api::handlers::user_auth))
        .route("/logout", post(api::handlers::logout))
        .route("/user", get(api::handlers::user_info))
        .route("/info", get(api::handlers::server_info))
        .route("/is_taken", get(api::handlers::check_free))
        .nest("/mods", Mod::routes())
        .nest("/versions", Version::routes())
        .nest("/mod_loaders", ModLoader::routes())
        .nest("/worlds", World::routes())
        .nest("/groups", Group::routes())
        .nest("/users", User::routes())
        .nest("/sessions", Session::routes())
        .nest("/invite_links", InviteLink::routes())
        .with_state(database.clone());

    let router = Router::new()
        .nest("/api", api)
        .route("/", get(|| async { "Hello!!" }))
        .layer(tower_http::trace::TraceLayer::new_for_http()
           .make_span_with(|request: &Request<_>| {
               // Log the matched route's path (with placeholders not filled in).
               // Use request.uri() or OriginalUri if you want the real path.
               let matched_path = request
                   .extensions()
                   .get::<MatchedPath>()
                   .map(MatchedPath::as_str);

               info_span!(
                "http_request",
                method = ?request.method(),
                matched_path,
                some_other_field = tracing::field::Empty,
            )
           })
        );


    let listener = tokio::net::TcpListener::bind(&config.listen_address).await?;


    axum::serve(listener, router).await?;

    Ok(())
}

#[test]
#[allow(unused)]
fn user_creation_and_removal() -> anyhow::Result<()> {
    const TEST_PORT: u32 = 3031;

    use crate::config;
    use crate::config::CONFIG;
    use crate::database::types::Id;
    use log::info;
    use pretty_assertions::assert_eq;
    use reqwest::header;
    use serde::Deserialize;
    use serde_with::serde_derive::Serialize;
    use std::thread;
    use axum::body::BodyDeserializeError;

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

    use crate::config;
    use crate::config::CONFIG;
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
