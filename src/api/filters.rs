use crate::api::util::rejections;
use crate::database::Database;
use crate::database::objects::{DbObject, Session, User};
use log::error;
use rusqlite::params;
use std::convert::Infallible;
use std::sync::{Arc, Mutex};
use warp::{Filter, Rejection};

pub fn with_db(
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
                    |err| {
                        eprintln!("{}", err);
                        Err(warp::reject::custom(rejections::InternalServerError))
                    },
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
                                |err| match err {
                                    rusqlite::Error::QueryReturnedNoRows => {
                                        Err(warp::reject::custom(rejections::Unauthorized))
                                    }
                                    _ => {
                                        eprintln!("{}", err);
                                        Err(warp::reject::custom(rejections::InternalServerError))
                                    },
                                },
                                |session| if let Ok(user) = database.get_one::<User>(session.user_id) { Ok(user) } else {
                                    eprintln!(
                                        "Orphaned session found, token: {}, user: {}. deleting (note: this should never happen because of SQLite foreign key requirement",
                                        session.token, session.user_id
                                    );
                                    match database.remove(&session) {
                                        Ok(_) => {}
                                        Err(error) => {
                                            eprintln!(
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
