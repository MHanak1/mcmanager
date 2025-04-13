use mcmanager::api::filters;
use mcmanager::api::handlers::{ApiCreate, ApiGet, ApiList, ApiUpdate};
use mcmanager::database::Database;
use mcmanager::database::objects::{InviteLink, Mod, ModLoader, Session, User, Version, World};
use mcmanager::{api, util};
use std::path::Path;
use std::sync::{Arc, Mutex};
use warp::Filter;

#[tokio::main]
async fn main() {
    let conn = rusqlite::Connection::open(Path::new(&util::dirs::data_dir().join("database.db")))
        .expect("failed to open database");
    let database = Database { conn };
    database.init().expect("failed to initialize database");
    run(database).await;
}

/// This is a piece of shit.
async fn run(database: Database) {
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
        .or(Mod::get_filter(db_mutex.clone()))
        .or(Mod::update_filter(db_mutex.clone()));
    let versions = Version::list_filter(db_mutex.clone())
        .or(Version::create_filter(db_mutex.clone()))
        .or(Version::get_filter(db_mutex.clone()))
        .or(Version::update_filter(db_mutex.clone()));
    let mod_loaders = ModLoader::list_filter(db_mutex.clone())
        .or(ModLoader::create_filter(db_mutex.clone()))
        .or(ModLoader::get_filter(db_mutex.clone()))
        .or(ModLoader::update_filter(db_mutex.clone()));
    let worlds = World::list_filter(db_mutex.clone())
        .or(World::create_filter(db_mutex.clone()))
        .or(World::get_filter(db_mutex.clone()))
        .or(World::update_filter(db_mutex.clone()));
    let users = User::list_filter(db_mutex.clone())
        .or(User::create_filter(db_mutex.clone()))
        .or(User::get_filter(db_mutex.clone()))
        .or(User::update_filter(db_mutex.clone()));
    let sessions = Session::list_filter(db_mutex.clone())
        .or(Session::create_filter(db_mutex.clone()))
        .or(Session::get_filter(db_mutex.clone()));
    let invite_links = InviteLink::list_filter(db_mutex.clone())
        .or(InviteLink::create_filter(db_mutex.clone()))
        .or(InviteLink::get_filter(db_mutex.clone()));

    warp::serve(
        login
            .or(user_info)
            .or(mods)
            .or(versions)
            .or(mod_loaders)
            .or(worlds)
            .or(users)
            .or(sessions)
            .or(invite_links),
    )
    .run(([127, 0, 0, 1], 3030))
    .await;
}
