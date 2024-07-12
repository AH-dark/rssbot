use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, serde::Serialize, serde::Deserialize)]
#[sea_orm(table_name = "subscriptions")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    #[sea_orm(default_expr = "Expr::current_timestamp()")]
    pub created_at: chrono::NaiveDateTime,

    #[sea_orm(not_null)]
    pub user_refer: i64,
    #[sea_orm(not_null)]
    pub target_chat: i64,
    #[sea_orm(not_null)]
    pub url: String,

    pub last_updated: chrono::NaiveDateTime,
    pub last_sent: Option<chrono::NaiveDateTime>,
    pub last_error: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::UserRefer",
        to = "super::user::Column::TelegramUserId"
    )]
    User,
}

impl Related<crate::user::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::User.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
