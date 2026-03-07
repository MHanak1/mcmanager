use sea_orm::entity::prelude::*;
use uuid::Uuid;

use crate::database::Loader;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "version")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: Uuid,

    #[sea_orm(unique, indexed)]
    pub slug: String,

    pub minecraft_version: String,

    pub loader: Loader,
}

impl ActiveModelBehavior for ActiveModel {
    fn new() -> Self {
        Self {
            id: sea_orm::Set(Uuid::now_v7()),
            ..ActiveModelTrait::default()
        }
    }
}
