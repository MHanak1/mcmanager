use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, FromJsonQueryResult)]
pub enum ExternalSource {
    Modrinth { id: String },
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "modification")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: Uuid,
    #[sea_orm(unique, indexed)]
    pub name: String,
    pub description: String,

    pub source_user_id: Option<Uuid>,
    #[sea_orm(belongs_to, from = "source_user_id", to = "id")]
    pub source_user: HasOne<super::user::Entity>,

    pub source_external: Option<ExternalSource>,

    #[sea_orm(has_many)]
    pub versions: HasMany<super::modification_version::Entity>,
}

impl ActiveModelBehavior for ActiveModel {
    fn new() -> Self {
        Self {
            id: sea_orm::Set(Uuid::now_v7()),
            ..ActiveModelTrait::default()
        }
    }
}
