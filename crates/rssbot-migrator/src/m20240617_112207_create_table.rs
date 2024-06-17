use sea_orm_migration::prelude::*;

use crate::sea_orm::Schema;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let schema = Schema::new(manager.get_database_backend());

        manager.create_table(schema.create_table_from_entity(rssbot_entities::user::Entity)).await?;
        manager.create_table(schema.create_table_from_entity(rssbot_entities::subscription::Entity)).await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.drop_table(Table::drop().table(rssbot_entities::subscription::Entity).if_exists().to_owned()).await?;
        manager.drop_table(Table::drop().table(rssbot_entities::user::Entity).if_exists().to_owned()).await?;

        Ok(())
    }
}

#[derive(DeriveIden)]
enum Post {
    Table,
    Id,
    Title,
    Text,
}
