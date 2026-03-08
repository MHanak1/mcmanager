use async_trait::async_trait;
use sea_orm::entity::prelude::*;
use smart_default::SmartDefault;
use uuid::Uuid;
use validator::Validate;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "user")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: Uuid,

    #[sea_orm(unique, indexed)]
    pub slug: String,

    #[sea_orm(default_value = "")]
    pub username: String,

    pub group_id: Uuid,

    #[sea_orm(belongs_to, from = "group_id", to = "id")]
    pub group: HasOne<super::group::Entity>,

    #[sea_orm(has_many)]
    pub invite_links: HasMany<super::invite_link::Entity>,

    #[sea_orm(has_many)]
    pub worlds: HasMany<super::world::Entity>,

    #[sea_orm(default_value = 0)]
    pub total_memory_usage: i64,

    #[sea_orm(default_value = true)]
    pub enabled: bool,
}

#[async_trait]
impl ActiveModelBehavior for ActiveModel {
    fn new() -> Self {
        Self {
            id: sea_orm::Set(Uuid::now_v7()),
            total_memory_usage: sea_orm::Set(0),
            enabled: sea_orm::Set(true),
            ..ActiveModelTrait::default()
        }
    }
}

#[derive(DerivePartialModel, Validate, SmartDefault)]
#[sea_orm(entity = "Entity", into_active_model)]
pub struct PartialUser {
    #[validate(length(min = 1), regex(path = *crate::util::RE_SLUG))]
    pub slug: String,

    #[validate(length(min = 3, max = 32))]
    pub username: String,

    pub group_id: Uuid,

    #[validate(range(min = 0))]
    pub total_memory_usage: i64,

    pub enabled: bool,
}
