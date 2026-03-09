use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use smart_default::SmartDefault;
use uuid::Uuid;
use validator::Validate;

use crate::database::StringVec;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "group")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: Uuid,

    #[sea_orm(unique, indexed)]
    pub slug: String,

    #[sea_orm(default_value = "")]
    pub name: String,

    #[sea_orm(has_many)]
    pub users: HasMany<super::user::Entity>,

    #[sea_orm(default_value = "{}")]
    pub limits: Limits,

    #[sea_orm(default_value = "{}")]
    pub permissions: Permissions,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, FromJsonQueryResult, Default)]
pub struct Limits {
    #[serde(default)]
    pub total_memory_limit: Option<i32>,

    #[serde(default)]
    pub per_world_memory_limit: Option<i32>,

    #[serde(default)]
    pub world_limit: Option<i32>,

    #[serde(default)]
    pub active_world_limit: Option<i32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, FromJsonQueryResult, Default)]
pub struct Permissions {
    #[serde(default)]
    pub config_blacklist: StringVec,

    #[serde(default)]
    pub config_whitelist: StringVec,

    #[serde(default)]
    pub can_upload_mods: bool,

    #[serde(default)]
    pub can_create_invites: bool,

    #[serde(default)]
    pub is_privileged: bool,
}

#[derive(DerivePartialModel, Validate, SmartDefault)]
#[sea_orm(entity = "Entity", into_active_model)]
pub struct PartialGroup {
    #[validate(length(min = 1), regex(path = *crate::util::RE_SLUG))]
    pub slug: String,

    pub name: String,

    pub limits: Limits,

    pub permissions: Permissions,
}

impl ActiveModelBehavior for ActiveModel {
    fn new() -> Self {
        Self {
            id: sea_orm::Set(Uuid::now_v7()),
            ..ActiveModelTrait::default()
        }
    }
}
