use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "subscriptions")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub created_at: chrono::NaiveDateTime,

    #[sea_orm(not_null)]
    pub user_refer: i64,
    #[sea_orm(not_null)]
    pub target_chat: i64,
    #[sea_orm(not_null)]
    pub url: String,

    #[sea_orm(default_value = "Null")]
    pub last_updated: Option<chrono::NaiveDateTime>,
    #[sea_orm(default_value = "Null")]
    pub last_sent: Option<chrono::NaiveDateTime>,
    #[sea_orm(default_value = "Null")]
    pub last_error: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
