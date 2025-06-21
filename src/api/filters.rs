use crate::api::util::rejections;
use crate::database::objects::{Session, User};
use crate::database::{Database, DatabaseError};
use log::error;
use std::convert::Infallible;
use std::sync::Arc;
use tokio::sync::Mutex;
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
        .or(warp::cookie("sessionToken"))
        .unify()
        .and_then(move |token: String| {
            let database = database.clone();
            async move {
                let database = database.lock().await;

                let session = database.list_filtered::<Session>(vec![("token".to_string(), token)], None)
                    .map_err(
                        |err|
                        match err {
                            DatabaseError::SqliteError(rusqlite::Error::QueryReturnedNoRows) => {
                                warp::reject::custom(rejections::Unauthorized)
                            }
                            _ => {
                                warp::reject::custom(rejections::InternalServerError::from(err.to_string()))
                            }
                        }
                    )?;
                let session = session.first().cloned();
                if session.is_none() {
                    return Err(warp::reject::custom(rejections::Unauthorized));
                }
                let session = session.unwrap();

                database.get_one::<User>(session.user_id, None).map_err(|err| {
                    error!("Orphaned session found, token: {}, user: {}. deleting (note: this should never happen because of SQLite foreign key requirement",session.token, session.user_id);
                        if let Err(err) = database.remove(&session, None) {
                            error!("Failed to remove orphaned session: {err}\n(what the fuck)");
                        };
                        warp::reject::custom(rejections::InternalServerError::from(err.to_string()))
                })
            }
        })
}
