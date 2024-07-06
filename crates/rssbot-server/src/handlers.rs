use teloxide::prelude::*;
use teloxide::utils::command::BotCommands;

#[derive(Debug, Clone, BotCommands)]
#[command(rename_rule = "lowercase", description = "These commands are supported:")]
/// Unstated commands
pub enum UnstatedCommand {
    #[command(description = "Display help message")]
    Help,
    #[command(description = "Subscribe to an RSS feed")]
    Subscribe,
    #[command(description = "Unsubscribe from an RSS feed")]
    Unsubscribe,
    #[command(description = "List all subscriptions")]
    List,
}

#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
pub enum State {
    #[default]
    Unstated,
}

#[tracing::instrument]
pub async fn handle_help(message: Message, bot: Bot) -> ResponseResult<()> {
    bot.send_message(message.chat.id, UnstatedCommand::descriptions().to_string()).await?;
    Ok(())
}
