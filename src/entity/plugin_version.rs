use sea_orm::entity::prelude::*;
use uuid::Uuid;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "plugin_version")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: Uuid,

    pub plugin_id: Uuid,

    #[sea_orm(belongs_to, from = "plugin_id", to = "id")]
    pub plugin: HasOne<super::plugin::Entity>,

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
