use crate::api::serve::AppState;
use crate::database::objects::{DbObject, Password, Session, User};
use crate::database::types::Id;
use crate::database::{Database, DatabaseError};
use argon2::password_hash::{Salt, SaltString};
use argon2::{PasswordHash, PasswordHasher, PasswordVerifier};
use log::debug;
use std::sync::Arc;
use uuid::Uuid;

pub async fn try_user_auth(
    username: &str,
    password: &str,
    state: AppState,
) -> Result<Session, DatabaseError> {
    let user: Result<User, _> = state
        .database
        .get_where("username", username.to_string(), None)
        .await;

    let argon2 = argon2::Argon2::default();

    //here we hash a random password, so no matter if provided username is correct or not it will take roughly the same time
    if user.is_err() {
        bollocks_hash();
        debug!("rejecting auth for user {username}, user not found");
        return Err(DatabaseError::Unauthorized);
    }

    let user = user?;

    if !user.enabled {
        debug!("rejecting auth for user {username}, user is disabled");
        bollocks_hash();
        return Err(DatabaseError::Unauthorized);
    }

    let user_password = state.database.get_one::<Password>(user.id, None).await?;

    let argon2 = argon2::Argon2::default();

    if argon2
        .verify_password(password.as_ref(), &user_password.hash.password_hash())
        .is_err()
    {
        debug!("rejecting auth for user {username}, password is invalid");
        return Err(DatabaseError::Unauthorized);
    }

    let new_session = Session {
        id: Id::new_random(),
        user_id: user.id,
        token: Uuid::new_v4(),
        created: chrono::offset::Utc::now(),
        expires: true,
    };

    //bypass perm check, we want all users to be able to log in
    state.database.insert(&new_session, None).await?;

    debug!("accepting auth for user {username}");

    Ok(new_session)
}

fn bollocks_hash() {
    let argon2 = argon2::Argon2::default();
    let _ = argon2.hash_password_into(b"RandomPassword", b"RandomSalt", &mut [0u8; 32]);
}

pub async fn get_user(token: Uuid, state: AppState) -> Result<User, DatabaseError> {
    /*
    let session = database
        .conn
        .query_row(
            &format!("SELECT * FROM {} WHERE token = ?1", Session::table_name(),),
            params![token],
            Session::from_row,
        )
        .map_err(DatabaseError::SqlxError)?;
     */

    let session: Session = state.database.get_session(token, None).await?;

    Ok(state.database.get_user(session.user_id, None).await?)
}
