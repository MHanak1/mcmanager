use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "data")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub key: String,
    #[sea_orm(default_value = "")]
    pub value: Json,
}

impl ActiveModelBehavior for ActiveModel {}
