use argon2::{
    Argon2, PasswordHasher,
    password_hash::{SaltString, rand_core::OsRng},
};
use color_eyre::{Result, eyre::bail};
use sea_orm::entity::prelude::*;
use uuid::Uuid;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "password")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: Uuid,

    #[sea_orm(unique, indexed)]
    pub user_id: Uuid,

    #[sea_orm(belongs_to, from = "user_id", to = "id")]
    pub user: HasOne<super::user::Entity>,

    pub hash: String,
}

impl ActiveModelBehavior for ActiveModel {
    fn new() -> Self {
        Self {
            id: sea_orm::Set(Uuid::now_v7()),
            ..ActiveModelTrait::default()
        }
    }
}

#[derive(DerivePartialModel)]
#[sea_orm(entity = "Entity", into_active_model)]
pub struct PartialPassword {
    pub user_id: Uuid,
    pub hash: String,
}

impl PartialPassword {
    pub fn new(user_id: Uuid, password: &str) -> Result<Self> {
        let salt = SaltString::generate(&mut OsRng);

        let argon2 = Argon2::default();

        let hash = argon2.hash_password(password.as_bytes(), &salt);

        let hash = match hash {
            Ok(hash) => hash.to_string(),
            Err(err) => bail!(err.to_string()), // argon2 uses some weir type to return error which confuses color_eyre
        };

        Ok(Self { user_id, hash })
    }
}
