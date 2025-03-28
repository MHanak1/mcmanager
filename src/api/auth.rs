use crate::database::objects::{DbObject, Password, Session, User};
use crate::database::types::Token;
use anyhow::{Result, anyhow};
use argon2::PasswordHasher;
use chrono::DateTime;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use crate::database;
use crate::database::Database;

pub fn try_user_auth(username: String, password: String, database: &Database) -> Result<Session> {
    let user = database.conn.query_row(
        &format!("SELECT * FROM {} WHERE name = ?1", User::table_name(),),
        params![username],
        User::from_row,
    )?;

    let user_password = Password::get_from_db(&database.conn, user.id)?;

    let argon2 = argon2::Argon2::default();
    
    let password_hash = argon2
        .hash_password(password.as_bytes(), &user_password.salt)
        .unwrap();

    if password_hash.to_string() != user_password.hash {
        return Err(anyhow!("Invalid password"));
    }

    let new_session = Session {
        user_id: user.id,
        token: Token::new(4),
        created: chrono::offset::Utc::now(),
        expires: false,
    };
    
    database.insert(&new_session)?;

    Ok(new_session)
}

pub fn get_user(token: String, conn: &Connection) -> Result<User> {
    let session = conn.query_row(
        &format!("SELECT * FROM {} WHERE token = ?1", Session::table_name(),),
        params![token],
        Session::from_row,
    )?;

    match User::get_from_db(&conn, session.user_id) {
        Ok(user) => Ok(user),
        Err(_) => Err(anyhow!("User not found")),
    }
}
