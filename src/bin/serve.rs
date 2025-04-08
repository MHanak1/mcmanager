use log::error;
use mcmanager::api::handlers::{ApiCreate, ApiGet, ApiList};
use mcmanager::api::util::rejections::InvalidBearerToken;
use mcmanager::database::Database;
use mcmanager::database::objects::{DbObject, Mod, ModLoader, Session, User, Version, World};
use mcmanager::{api, util};
use rusqlite::params;
use std::convert::Infallible;
use std::path::Path;
use std::string::String;
use std::sync::{Arc, Mutex};
use warp::{Filter, Rejection};

#[tokio::main]
async fn main() {
    let conn =
        rusqlite::Connection::open(Path::new(&util::dirs::data_dir().join("database.db"))).unwrap();
    let database = Database { conn };
    database.init().unwrap();
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

    //LIST
    let list_mods = warp::get()
        .and(warp::path!("api" / "mods"))
        .and(warp::path::end())
        .and(filters::with_db(db_mutex.clone()))
        //.and(with_session(session_store.clone()))
        .and(filters::with_auth(db_mutex.clone()))
        .and_then(Mod::api_list);

    let list_versions = warp::get()
        .and(warp::path!("api" / "versions"))
        .and(warp::path::end())
        .and(filters::with_db(db_mutex.clone()))
        //.and(with_session(session_store.clone()))
        .and(filters::with_auth(db_mutex.clone()))
        .and_then(Version::api_list);

    let list_mod_loaders = warp::get()
        .and(warp::path!("api" / "loaders"))
        .and(warp::path::end())
        .and(filters::with_db(db_mutex.clone()))
        //.and(with_session(session_store.clone()))
        .and(filters::with_auth(db_mutex.clone()))
        .and_then(ModLoader::api_list);

    let list_worlds = warp::get()
        .and(warp::path!("api" / "worlds"))
        .and(warp::path::end())
        .and(filters::with_db(db_mutex.clone()))
        //.and(with_session(session_store.clone()))
        .and(filters::with_auth(db_mutex.clone()))
        .and_then(World::api_list);

    let list_users = warp::get()
        .and(warp::path!("api" / "users"))
        .and(warp::path::end())
        .and(filters::with_db(db_mutex.clone()))
        //.and(with_session(session_store.clone()))
        .and(filters::with_auth(db_mutex.clone()))
        .and_then(User::api_list);

    //GET
    let get_mod = warp::get()
        .and(warp::path!("api" / "mods" / String))
        .and(warp::path::end())
        .and(filters::with_db(db_mutex.clone()))
        .and(filters::with_auth(db_mutex.clone()))
        .and_then(Mod::api_get);

    let get_version = warp::get()
        .and(warp::path!("api" / "versions" / String))
        .and(warp::path::end())
        .and(filters::with_db(db_mutex.clone()))
        .and(filters::with_auth(db_mutex.clone()))
        .and_then(Version::api_get);

    let get_mod_loader = warp::get()
        .and(warp::path!("api" / "loaders" / String))
        .and(warp::path::end())
        .and(filters::with_db(db_mutex.clone()))
        .and(filters::with_auth(db_mutex.clone()))
        .and_then(ModLoader::api_get);

    let get_world = warp::get()
        .and(warp::path!("api" / "worlds" / String))
        .and(warp::path::end())
        .and(filters::with_db(db_mutex.clone()))
        .and(filters::with_auth(db_mutex.clone()))
        .and_then(World::api_get);

    let get_user = warp::get()
        .and(warp::path!("api" / "users" / String))
        .and(warp::path::end())
        .and(filters::with_db(db_mutex.clone()))
        .and(filters::with_auth(db_mutex.clone()))
        .and_then(User::api_get);

    //CREATE
    let create_mod = warp::post()
        .and(warp::path!("api" / "mods" / "create"))
        .and(warp::path::end())
        .and(filters::with_db(db_mutex.clone()))
        .and(filters::with_auth(db_mutex.clone()))
        .and(warp::body::json())
        .and_then(Mod::api_create);

    let create_version = warp::post()
        .and(warp::path!("api" / "versions" / "create"))
        .and(warp::path::end())
        .and(filters::with_db(db_mutex.clone()))
        .and(filters::with_auth(db_mutex.clone()))
        .and(warp::body::json())
        .and_then(Version::api_create);

    let create_mod_loader = warp::post()
        .and(warp::path!("api" / "loaders" / "create"))
        .and(warp::path::end())
        .and(filters::with_db(db_mutex.clone()))
        .and(filters::with_auth(db_mutex.clone()))
        .and(warp::body::json())
        .and_then(ModLoader::api_create);

    let create_world = warp::post()
        .and(warp::path!("api" / "worlds" / "create"))
        .and(warp::path::end())
        .and(filters::with_db(db_mutex.clone()))
        .and(filters::with_auth(db_mutex.clone()))
        .and(warp::body::json())
        .and_then(World::api_create);

    let create_user = warp::post()
        .and(warp::path!("api" / "users" / "create"))
        .and(warp::path::end())
        .and(filters::with_db(db_mutex.clone()))
        .and(filters::with_auth(db_mutex.clone()))
        .and(warp::body::json())
        .and_then(User::api_create);

    warp::serve(
        //set_cookie
        login
            .or(user_info)
            .or(create_mod)
            .or(create_version)
            .or(create_mod_loader)
            .or(create_world)
            .or(create_user)
            .or(list_mods)
            .or(list_versions)
            .or(list_mod_loaders)
            .or(list_worlds)
            .or(list_users)
            .or(get_mod)
            .or(get_version)
            .or(get_mod_loader)
            .or(get_world)
            .or(get_user),
    )
    //  .or(set_cookie)
    .run(([127, 0, 0, 1], 3030))
    .await;
}

mod filters {
    use log::error;
    use mcmanager::api::util::rejections;
    use mcmanager::database::Database;
    use mcmanager::database::objects::{DbObject, Session, User};
    use rusqlite::params;
    use std::convert::Infallible;
    use std::sync::{Arc, Mutex};
    use warp::{Filter, Rejection};

    pub(crate) fn with_db(
        db: Arc<Mutex<Database>>,
    ) -> impl Filter<Extract = (Arc<Mutex<Database>>,), Error = Infallible> + Clone {
        warp::any().map(move || Arc::clone(&db))
    }

    pub fn with_bearer_token() -> impl Filter<Extract = (String,), Error = Rejection> + Clone {
        warp::header::<String>("Authorization").and_then(|header: String| async move {
            if header[0..7] == *"Bearer " {
                Ok(header[7..].to_string())
            } else {
                Err(warp::reject::custom(rejections::InvalidBearerToken))
            }
        })
    }
    pub fn with_auth(
        database: Arc<Mutex<Database>>,
    ) -> impl Filter<Extract = (User,), Error = Rejection> + Clone {
        with_bearer_token()
            .or(warp::cookie("auth"))
            .unify()
            .and_then(move |token: String| {
                let database = database.clone();
                async move {
                    database.lock().map_or_else(
                        |_| Err(warp::reject::custom(rejections::InternalServerError)),
                        |database| {
                            database
                                .conn
                                .query_row(
                                    &format!(
                                        "SELECT * FROM {} WHERE token = ?1",
                                        Session::table_name(),
                                    ),
                                    params![token],
                                    Session::from_row,
                                )
                                .map_or_else(
                                    |error| match error {
                                        rusqlite::Error::QueryReturnedNoRows => {
                                            Err(warp::reject::custom(rejections::Unauthorized))
                                        }
                                        _ => Err(warp::reject::custom(rejections::InternalServerError)),
                                    },
                                    |session| if let Ok(user) = database.get_one::<User>(session.user_id) { Ok(user) } else {
                                        error!(
                                            "Orphaned session found, token: {}, user: {}. deleting (note: this should never happen because of SQLite foreign key requirement",
                                            session.token, session.user_id
                                        );
                                        match database.remove(&session) {
                                            Ok(_) => {}
                                            Err(error) => {
                                                error!(
                                                    "Failed to remove orphaned session: {}\n(what the fuck)",
                                                    error
                                                );
                                            }
                                        }
                                        Err(warp::reject::custom(rejections::InternalServerError))
                                    },
                                )
                        },
                    )
                }
            })
    }
}
