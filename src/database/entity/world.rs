use sea_orm::entity::prelude::*;
use uuid::Uuid;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "world")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: Uuid,

    #[sea_orm(default_value = "")]
    pub name: String,

    //use hostname as the slug
    #[sea_orm(unique, indexed, default_value = "")]
    pub hostname: String,

    pub allocated_memory: i32,

    pub owner_id: Uuid,

    #[sea_orm(belongs_to, from = "owner_id", to = "id")]
    pub owner: HasOne<super::user::Entity>,

    pub version_id: Uuid,

    #[sea_orm(belongs_to, from = "version_id", to = "id")]
    pub version: HasOne<super::version::Entity>,
}

impl ActiveModelBehavior for ActiveModel {
    fn new() -> Self {
        Self {
            id: sea_orm::Set(Uuid::now_v7()),
            ..ActiveModelTrait::default()
        }
    }
}
