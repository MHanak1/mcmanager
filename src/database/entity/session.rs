use chrono::{DateTime, Utc};
use sea_orm::entity::prelude::*;
use smart_default::SmartDefault;
use uuid::Uuid;
use validator::Validate;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "session")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: Uuid,

    #[sea_orm(uniqie)]
    pub user_id: Uuid,

    #[sea_orm(belongs_to, from = "user_id", to = "id")]
    pub user: HasOne<super::user::Entity>,

    #[seaography(ignore)]
    pub token: Uuid,

    pub created: DateTime<Utc>,

    #[sea_orm(default_value = true)]
    pub expires: bool,
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

#[derive(DerivePartialModel, Validate, SmartDefault)]
#[sea_orm(entity = "Entity", into_active_model)]
pub struct PartialSession {
    #[sea_orm(uniqie)]
    pub user_id: Uuid,

    #[default(Utc::now())]
    pub created: DateTime<Utc>,

    #[sea_orm(default_value = true)]
    pub expires: bool,
}
