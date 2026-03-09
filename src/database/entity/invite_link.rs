use sea_orm::entity::prelude::*;
use uuid::Uuid;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "invite_link")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: Uuid,

    #[seaography(ignore)]
    #[sea_orm(unique, indexed)]
    pub token: uuid::Uuid,

    pub creator_id: Uuid,

    #[sea_orm(belongs_to, from = "creator_id", to = "id")]
    pub creator: HasOne<super::user::Entity>,

    pub created: DateTime,
}

impl ActiveModelBehavior for ActiveModel {
    fn new() -> Self {
        Self {
            id: sea_orm::Set(Uuid::now_v7()),
            token: sea_orm::Set(Uuid::new_v4()),
            ..ActiveModelTrait::default()
        }
    }
}
