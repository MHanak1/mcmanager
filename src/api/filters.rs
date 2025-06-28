use crate::api::util::rejections;
use crate::database;
use crate::database::objects::{Session, User};
use crate::database::{Database, DatabaseError};
use log::error;
use std::convert::Infallible;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;
use warp::{Filter, Rejection};

pub fn with_db(
    db: Arc<Database>,
) -> impl Filter<Extract = (Arc<Database>,), Error = Infallible> + Clone {
    warp::any().map(move || db.clone())
}

pub fn with_bearer_token() -> impl Filter<Extract = (Uuid,), Error = Rejection> + Clone {
    warp::header::<String>("Authorization").and_then(|header: String| async move {
        if header[0..7] == *"Bearer " {
            Ok(Uuid::parse_str(&header[7..]).map_err(|_| rejections::InvalidBearerToken)?)
        } else {
            Err(warp::reject::custom(rejections::InvalidBearerToken))
        }
    })
}
pub fn with_auth(
    database: Arc<Database>,
) -> impl Filter<Extract = (User,), Error = Rejection> + Clone {
    with_bearer_token()
        .or(warp::cookie("sessionToken"))
        .unify()
        .and_then(move |token: Uuid| {
            let database = database.clone();
            async move {
                let session = database.get_where::<Session, _>("token", token, None).await;
                if session.is_err() {
                    return Err(warp::reject::custom(rejections::Unauthorized));
                }
                let session = session.unwrap();

                Ok(database
                    .get_one::<User>(session.user_id, None)
                    .await
                    .map_err(|err| {
                        warp::reject::custom(rejections::InternalServerError::from(err.to_string()))
                    })?)
            }
        })
}
