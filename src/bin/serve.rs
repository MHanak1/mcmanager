use std::convert::Infallible;
use std::string::String;
use mcmanager::database::Database;
use mcmanager::{api, util};
use std::path::Path;
use std::sync::{Arc, Mutex};
use log::error;
use rusqlite::fallible_iterator::FallibleIterator;
use rusqlite::params;
use warp::{Filter, Rejection};
use mcmanager::database::objects::{DbObject, Session, User};

#[tokio::main]
async fn main() {
    let conn =
        rusqlite::Connection::open(Path::new(&util::dirs::data_dir().join("database.db"))).unwrap();
    let database = Database { conn };
    database.init().unwrap();
    run(database).await;
}

async fn run(database: Database) {
    // GET /hello/warp => 200 OK with body "Hello, warp!"
    util::dirs::init_dirs().expect("Failed to initialize the data directory");

    let db_mutex = Arc::new(Mutex::new(database));

    //let header = warp::header::optional::<String>("user_token");

    let list_mods = warp::get()
        .and(warp::path!("mods"))
        .and(warp::path::end())
        .and(with_db(db_mutex.clone()))
        //.and(with_session(session_store.clone()))
        .and(with_auth(db_mutex.clone()))
        .and_then(api::handlers::list_mods);

    let get_mod = warp::get()
        .and(warp::path!("mods" / String))
        .and(warp::path::end())
        .and(with_db(db_mutex.clone()))
        .and(with_auth(db_mutex.clone()))
        .and_then(api::handlers::get_mod);
    
    let list_worlds = warp::get()
        .and(warp::path!("worlds"))
        .and(warp::path::end())
        .and(with_db(db_mutex.clone()))
        //.and(with_session(session_store.clone()))
        .and(with_auth(db_mutex.clone()))
        .and_then(api::handlers::list_worlds);

    warp::serve(
        //set_cookie
        list_mods
        .or(get_mod)
        .or(list_worlds))
        //  .or(set_cookie)
        .run(([127, 0, 0, 1], 3030))
        .await;
}

fn with_db(
    db: Arc<Mutex<Database>>,
) -> impl Filter<Extract = (Arc<Mutex<Database>>,), Error = Infallible> + Clone {
    warp::any().map(move || Arc::clone(&db))
}

fn with_auth(database: Arc<Mutex<Database>>)  -> impl Filter<Extract = (Option<User>,), Error = Rejection> + Clone {
    warp::header::optional::<String>("auth").map(move |header: Option<String>| {
        match database.lock() {
            Ok(database) => {
                if header.is_none() {
                    println!("No auth header found");
                }
                header.as_ref()?;
                let header = header.unwrap();
                match database.conn.query_row(
                    &format!(
                        "SELECT * FROM {} WHERE token = ?1",
                        Session::table_name(),
                    ),
                    params![header],
                    Session::from_row,
                ) {
                    Ok(session) => {
                        match database.get_one::<User>(session.user_id) {
                            Ok(user) => {
                                Some(user)
                            }
                            Err(err) => {
                                error!("Orphaned session found, token: {}, user: {}. deleting", session.token, session.user_id);
                                match database.remove(&session) {
                                    Ok(_) => {},
                                    Err(error) => {
                                        error!("Failed to remove orphaned session: {}", error);
                                    }
                                }
                                None
                            }
                        }
                    },
                    Err(err) => {
                        None
                    },
                }
            }
            Err(err) => {
                error!("failed to lock database: {:?}", err);
                None
            }
        }
    })
}
