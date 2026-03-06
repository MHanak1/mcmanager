use sea_orm::entity::prelude::*;
use uuid::Uuid;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "user")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: Uuid,

    pub username: String,

    pub group_id: Uuid,

    #[sea_orm(belongs_to, from = "group_id", to = "id")]
    pub group: HasOne<super::group::Entity>,

    #[sea_orm(has_one)]
    pub password: HasOne<super::password::Entity>,

    #[sea_orm(has_many)]
    pub invite_links: HasMany<super::invite_link::Entity>,

    pub total_memory_usage: i64,

    pub enabled: bool,
}

impl ActiveModelBehavior for ActiveModel {
    fn new() -> Self {
        Self {
            id: sea_orm::Set(Uuid::now_v7()),
            ..ActiveModelTrait::default()
        }
    }
}
