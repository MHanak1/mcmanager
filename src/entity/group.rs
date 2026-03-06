use sea_orm::entity::prelude::*;
use uuid::Uuid;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "group")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: Uuid,

    #[sea_orm(unique, indexed)]
    pub name: String,

    pub total_memory_limit: Option<i32>,

    pub per_world_memory_limit: Option<i32>,

    pub world_limit: Option<i32>,

    pub active_world_limit: Option<i32>,

    pub storage_limit: Option<i32>,

    #[sea_orm(column_type = "JsonBinary")]
    pub config_blacklist: Vec<String>,

    #[sea_orm(column_type = "JsonBinary")]
    pub config_whitelist: Vec<String>,

    pub can_upload_mods: bool,

    pub is_privileged: bool,

    #[sea_orm(has_many)]
    pub users: HasMany<super::user::Entity>,
}

impl ActiveModelBehavior for ActiveModel {
    fn new() -> Self {
        Self {
            id: sea_orm::Set(Uuid::now_v7()),
            ..ActiveModelTrait::default()
        }
    }
}
