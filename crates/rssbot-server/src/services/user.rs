use sea_orm::ActiveValue;
use sea_orm::prelude::*;

use rssbot_entities::user;

#[derive(Clone, Debug)]
pub(crate) struct Service {
    db: DatabaseConnection,
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Database error: {0}")]
    DatabaseError(#[from] DbErr),
}

impl Service {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    #[tracing::instrument]
    pub async fn create_or_find_user(&self, user_id: i64, username: String) -> Result<user::Model, Error> {
        if let Some(user) = user::Entity::find_by_id(user_id).one(&self.db).await? {
            return Ok(user);
        }

        let user = user::ActiveModel {
            telegram_user_id: ActiveValue::Set(user_id),
            username: ActiveValue::Set(username),
        }
            .insert(&self.db).await?;

        Ok(user)
    }
}
