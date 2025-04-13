use crate::database::Database;
use crate::database::objects::{DbObject, Password, Session, User};
use crate::database::types::Token;
use anyhow::{Result, anyhow};
use argon2::PasswordHasher;
use rusqlite::params;

pub fn try_user_auth(username: &str, password: &str, database: &Database) -> Result<Session> {
    let user = database.conn.query_row(
        &format!("SELECT * FROM {} WHERE name = ?1", User::table_name(),),
        params![username],
        User::from_row,
    );

    let argon2 = argon2::Argon2::default();

    //here we hash a random password, so no matter if provided username is correct or not it will take roughly the same time
    if user.is_err() {
        bollocks_hash();
        return Err(anyhow!("Invalid username or password"));
    }

    let user = user?;

    if !user.enabled {
        bollocks_hash();
        return Err(anyhow!("User disabled"));
    }

    let user_password = database.get_one::<Password>(user.id, None)?;

    let password_hash = argon2
        .hash_password(password.as_bytes(), &user_password.salt)
        .expect("failed to hash password");

    if password_hash.to_string() != user_password.hash {
        return Err(anyhow!("Invalid username or password"));
    }

    let new_session = Session {
        user_id: user.id,
        token: Token::new(4),
        created: chrono::offset::Utc::now(),
        expires: true,
    };

    //bypass perm check, we want all users to be able to log in
    database.insert(&new_session, None)?;

    Ok(new_session)
}

fn bollocks_hash() {
    let argon2 = argon2::Argon2::default();
    let _ = argon2.hash_password_into(b"RandomPassword", b"RandomSalt", &mut [0u8; 32]);
}

pub fn get_user(token: &str, database: &Database) -> Result<User> {
    let session = database.conn.query_row(
        &format!("SELECT * FROM {} WHERE token = ?1", Session::table_name(),),
        params![token],
        Session::from_row,
    )?;

    match database.get_one::<User>(session.user_id, None) {
        Ok(user) => Ok(user),
        Err(_) => Err(anyhow!("User not found")),
    }
}
