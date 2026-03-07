use sea_orm::entity::prelude::*;
use uuid::Uuid;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "modification_version")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: Uuid,

    pub modification_id: Uuid,

    #[sea_orm(belongs_to, from = "modification_id", to = "id")]
    pub modification: HasOne<super::modification::Entity>,

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
