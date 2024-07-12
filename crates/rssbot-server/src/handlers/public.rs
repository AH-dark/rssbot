use std::sync::Arc;

use redis::aio::MultiplexedConnection;
use redis::AsyncCommands;
use teloxide::prelude::*;
use teloxide::utils::command::BotCommands;

use crate::data::SelectChatSessionData;
use crate::services;

#[derive(Debug, Clone, BotCommands)]
#[command(rename_rule = "lowercase", description = "These commands are supported:")]
/// Unstated commands
pub enum Command {
    #[command(description = "Handle subscribe waiting target chat state start")]
    Start { id: String },
    #[command(description = "Display help message")]
    Help,
}

#[tracing::instrument]
pub async fn handle_start(bot: Bot, message: Message, mut redis_con: MultiplexedConnection, service: Arc<services::subscription::Service>, command: Command) -> anyhow::Result<()> {
    let id = &(match command {
        Command::Start { id } => id,
        _ => {
            bot.send_message(message.chat.id, "Invalid start command").await?;
            return Ok(());
        }
    });

    let sess_data: SelectChatSessionData = {
        let result = redis_con.get(id).await;
        // delete redis key immediately
        redis_con.del(id).await?;

        let record: Option<String> = match result {
            Ok(record) => record,
            Err(err) => {
                bot.send_message(message.chat.id, "Server internal error").await?;
                return Err(err.into());
            }
        };

        // check if session data exists
        if record.is_none() {
            bot.send_message(message.chat.id, "Session data not found").await?;
            return Ok(());
        }

        match serde_json::from_str(&record.unwrap()) {
            Ok(sess_data) => sess_data,
            Err(err) => {
                bot.send_message(message.chat.id, "Invalid session data").await?;
                return Err(err.into());
            }
        }
    };

    match service.add_subscription(sess_data.user_id, message.chat.id.0, sess_data.target_url).await {
        Ok(subscription) => {
            // notify chat
            if message.chat.is_group() || message.chat.is_supergroup() {
                bot.send_message(message.chat.id, format!("I will sync the feed and send updates to this chat: {}", subscription.url))
                    .disable_web_page_preview(true)
                    .await
                    .ok();
            }

            // notify user
            bot.send_message(UserId(sess_data.user_id as u64), format!("Subscription has been added to chat {}, url: {}", message.chat.id, subscription.url)).await?;

            log::info!("Subscription has been added: {:?}", subscription);
        }
        Err(err) => {
            bot.send_message(UserId(sess_data.user_id as u64), format!("Failed to add subscription: {}", err)).await?;
            log::error!("Failed to add subscription: {}", err);
            return Err(err.into());
        }
    };

    Ok(())
}

#[tracing::instrument]
pub async fn handle_unstated_help(bot: Bot, message: Message) -> anyhow::Result<()> {
    bot.send_message(message.chat.id, "You need to use this command in private message to see what I can do.").await?;
    Ok(())
}
