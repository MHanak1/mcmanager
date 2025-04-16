use mcmanager::api::filters;
use mcmanager::api::handlers::{ApiCreate, ApiGet, ApiList, ApiRemove, ApiUpdate};
use mcmanager::configuration;
use mcmanager::database::Database;
use mcmanager::database::objects::{InviteLink, Mod, ModLoader, Session, User, Version, World};
use mcmanager::{api, util};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::Path;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use warp::Filter;

#[tokio::main]
async fn main() {
    env_logger::init();
    let conn = rusqlite::Connection::open(Path::new(&util::dirs::data_dir().join("database.db")))
        .expect("failed to open the database");
    let database = Database { conn };
    database.init().expect("failed to initialize database");
    run(database).await;
}

/// This is a piece of shit.
async fn run(database: Database) {
    let listen_address = configuration::CONFIG
        .get::<String>("listen_address")
        .expect("invalid listen_address");
    let listen_port = configuration::CONFIG
        .get::<u16>("listen_port")
        .expect("invalid listen_port");

    // GET /hello/warp => 200 OK with body "Hello, warp!"
    util::dirs::init_dirs().expect("Failed to initialize the data directory");

    let db_mutex = Arc::new(Mutex::new(database));

    //let header = warp::header::optional::<String>("user_token");

    //LOGIN
    let login = warp::post()
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

    let mods = Mod::list_filter(db_mutex.clone())
        .or(Mod::create_filter(db_mutex.clone()))
        .or(Mod::update_filter(db_mutex.clone()))
        .or(Mod::get_filter(db_mutex.clone()))
        .or(Mod::remove_filter(db_mutex.clone()));
    let versions = Version::list_filter(db_mutex.clone())
        .or(Version::create_filter(db_mutex.clone()))
        .or(Version::update_filter(db_mutex.clone()))
        .or(Version::get_filter(db_mutex.clone()))
        .or(Version::remove_filter(db_mutex.clone()));
    let mod_loaders = ModLoader::list_filter(db_mutex.clone())
        .or(ModLoader::create_filter(db_mutex.clone()))
        .or(ModLoader::update_filter(db_mutex.clone()))
        .or(ModLoader::get_filter(db_mutex.clone()))
        .or(ModLoader::remove_filter(db_mutex.clone()));
    let worlds = World::list_filter(db_mutex.clone())
        .or(World::create_filter(db_mutex.clone()))
        .or(World::update_filter(db_mutex.clone()))
        .or(World::get_filter(db_mutex.clone()))
        .or(World::remove_filter(db_mutex.clone()));
    let users = User::list_filter(db_mutex.clone())
        .or(User::create_filter(db_mutex.clone()))
        .or(User::update_filter(db_mutex.clone()))
        .or(User::get_filter(db_mutex.clone()))
        .or(User::remove_filter(db_mutex.clone()));
    let sessions = Session::list_filter(db_mutex.clone())
        .or(Session::create_filter(db_mutex.clone()))
        .or(Session::get_filter(db_mutex.clone()))
        .or(Session::remove_filter(db_mutex.clone()));
    let invite_links = InviteLink::list_filter(db_mutex.clone())
        .or(InviteLink::create_filter(db_mutex.clone()))
        .or(InviteLink::get_filter(db_mutex.clone()))
        .or(InviteLink::remove_filter(db_mutex.clone()));

    //let log = warp::log::custom(|info| info!("{} - {}: {}", info.method(), info.status(), info.path()));
    let log = warp::log("info");

    warp::serve(
        login
            .or(user_info)
            .or(mods)
            .or(versions)
            .or(mod_loaders)
            .or(worlds)
            .or(users)
            .or(sessions)
            .or(invite_links)
            .recover(api::handlers::handle_rejection)
            .with(log),
    )
    .run(SocketAddr::new(
        IpAddr::V4(Ipv4Addr::from_str(&listen_address).expect("invalid listen_address")),
        listen_port,
    ))
    .await;
}

#[test]
#[allow(unused)]
fn object_creation_and_removal() -> anyhow::Result<()> {
    use mcmanager::configuration;
    use pretty_assertions::assert_eq;
    use reqwest::header;
    use serde::Deserialize;
    use std::thread;
    env_logger::init();

    let conn = rusqlite::Connection::open_in_memory().expect("Can't open database connection");

    let database = Database { conn };
    database.init().expect("Can't init database");
    let mut admin = database.create_user("Admin", "Password1")?;
    admin.is_privileged = true;
    database.update(&admin, None)?;

    thread::spawn(|| {
        let rt = tokio::runtime::Runtime::new().expect("Can't create runtime");
        rt.block_on(run(database))
    });

    //wait for the server to start
    thread::sleep(std::time::Duration::from_secs(1));

    let url = format!(
        "http://{}:{}/api",
        configuration::CONFIG
            .get::<String>("listen_address")
            .expect("invalid listen_address"),
        configuration::CONFIG
            .get::<String>("listen_port")
            .expect("invalid listen_port")
    );

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
            .body("{\"name\": \"User1\", \"password\": \"Password2\"}")
            .send()?
            .text()?,
    )?;

    let user2: User = serde_json::from_str(
        &client
            .post(format!("{url}/users"))
            .header(header::AUTHORIZATION, format!("Bearer {admin_token}"))
            .body("{\"name\": \"User2\", \"password\": \"Password3\"}")
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
