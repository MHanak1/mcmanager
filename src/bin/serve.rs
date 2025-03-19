use crate::rejections::InvalidBearerToken;
use log::error;
use mcmanager::database::Database;
use mcmanager::database::objects::{DbObject, Session, User};
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

async fn run(database: Database) {
    // GET /hello/warp => 200 OK with body "Hello, warp!"
    util::dirs::init_dirs().expect("Failed to initialize the data directory");

    let db_mutex = Arc::new(Mutex::new(database));

    //let header = warp::header::optional::<String>("user_token");

    let list_mods = warp::get()
        .and(warp::path!("api" / "mods"))
        .and(warp::path::end())
        .and(with_db(db_mutex.clone()))
        //.and(with_session(session_store.clone()))
        .and(with_auth(db_mutex.clone()))
        .and_then(api::handlers::list_mods);

    let get_mod = warp::get()
        .and(warp::path!("api" / "mods" / String))
        .and(warp::path::end())
        .and(with_db(db_mutex.clone()))
        .and(with_auth(db_mutex.clone()))
        .and_then(api::handlers::get_mod);

    let list_worlds = warp::get()
        .and(warp::path!("api" / "worlds"))
        .and(warp::path::end())
        .and(with_db(db_mutex.clone()))
        //.and(with_session(session_store.clone()))
        .and(with_auth(db_mutex.clone()))
        .and_then(api::handlers::list_worlds);

    warp::serve(
        //set_cookie
        list_mods.or(get_mod).or(list_worlds),
    )
    //  .or(set_cookie)
    .run(([127, 0, 0, 1], 3030))
    .await;
}

fn with_db(
    db: Arc<Mutex<Database>>,
) -> impl Filter<Extract = (Arc<Mutex<Database>>,), Error = Infallible> + Clone {
    warp::any().map(move || Arc::clone(&db))
}

fn with_bearer_token() -> impl Filter<Extract = (String,), Error = Rejection> + Clone {
    warp::header::<String>("Authorization").and_then(|header: String| async move {
        if header[0..7] == *"Bearer " {
            Ok(header[7..].to_string())
        } else {
            Err(warp::reject::custom(InvalidBearerToken))
        }
    })
}

fn with_auth(
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
                                |session| match database.get_one::<User>(session.user_id) {
                                    Ok(user) => Ok(user),
                                    Err(_) => {
                                        error!(
                                            "Orphaned session found, token: {}, user: {}. deleting",
                                            session.token, session.user_id
                                        );
                                        match database.remove(&session) {
                                            Ok(_) => {}
                                            Err(error) => {
                                                error!(
                                                    "Failed to remove orphaned session: {}",
                                                    error
                                                );
                                            }
                                        }
                                        Err(warp::reject::custom(rejections::InternalServerError))
                                    }
                                },
                            )
                    },
                )
            }
        })
}

mod rejections {
    use warp::reject::Reject;

    #[derive(Debug)]
    pub struct InternalServerError;
    impl Reject for InternalServerError {}

    #[derive(Debug)]
    pub struct InvalidBearerToken;
    impl Reject for InvalidBearerToken {}

    #[derive(Debug)]
    pub struct Unauthorized;
    impl Reject for Unauthorized {}
}
