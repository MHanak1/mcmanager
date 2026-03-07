use sea_orm::entity::prelude::*;
use uuid::Uuid;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "plugin")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: Uuid,

    #[sea_orm(unique, indexed)]
    pub slug: String,

    #[sea_orm(default_value = "")]
    pub name: String,

    #[sea_orm(default_value = "")]
    pub description: String,

    #[sea_orm(has_many)]
    pub versions: HasMany<super::plugin_version::Entity>,
}

impl ActiveModelBehavior for ActiveModel {
    fn new() -> Self {
        Self {
            id: sea_orm::Set(Uuid::now_v7()),
            ..ActiveModelTrait::default()
        }
    }
}
