use std::sync::Arc;

use anyhow::anyhow;
use teloxide::dispatching::dialogue::{RedisStorage, serializer};
use teloxide::prelude::*;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardButtonKind, InlineKeyboardMarkup, ParseMode};
use teloxide::types::ReplyMarkup::InlineKeyboard;
use teloxide::utils::command::BotCommands;

use crate::services::{subscription, user};

#[derive(Debug, Clone, BotCommands)]
#[command(rename_rule = "lowercase", description = "These commands are supported:")]
/// Unstated commands
pub enum UnstatedCommand {
    #[command(description = "Start the bot")]
    Start,
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
#[serde(rename_all = "snake_case", tag = "state", content = "data")]
pub enum State {
    #[default]
    Unstated,
    SubscribeWaitingTargetChat,
    SubscribeWaitingUrl { target_chat: i64 },
    UnsubscribeWaitingCallbackQuery,
}

type BotDialog = Dialogue<State, RedisStorage<serializer::Json>>;

#[tracing::instrument]
pub async fn handle_start(message: Message, bot: Bot, user_service: Arc<user::Service>) -> anyhow::Result<()> {
    bot.send_message(message.chat.id, r#"Hello! I'm an RSS bot. Use the /help command to see what I can do."#).await?;


    let (user_id, username) = match message.from() {
        Some(user) => {
            let username = match user.username.as_ref() {
                Some(username) => username.clone(),
                None if user.last_name.as_ref().is_some() => format!("{} {}", user.first_name, user.last_name.clone().unwrap_or_default()),
                None => user.first_name.clone(),
            };

            (user.id.0 as i64, username)
        }
        None => {
            bot.send_message(message.chat.id, "User ID not found, failed to create user").await?;
            return Ok(());
        }
    };

    match user_service.create_or_find_user(user_id, username).await {
        Ok(_) => {}
        Err(e) => {
            bot.send_message(message.chat.id, e.to_string()).await?;
            tracing::error!("Failed to create or find user: {}", e);
            return Err(e.into());
        }
    }

    Ok(())
}

#[tracing::instrument]
pub async fn handle_help(message: Message, bot: Bot, user_service: Arc<user::Service>) -> anyhow::Result<()> {
    bot.send_message(message.chat.id, UnstatedCommand::descriptions().to_string()).await?;

    let (user_id, username) = match message.from() {
        Some(user) => {
            let username = match user.username.as_ref() {
                Some(username) => username.clone(),
                None if user.last_name.as_ref().is_some() => format!("{} {}", user.first_name, user.last_name.clone().unwrap_or_default()),
                None => user.first_name.clone(),
            };

            (user.id.0 as i64, username)
        }
        None => {
            bot.send_message(message.chat.id, "User ID not found, failed to create user").await?;
            return Ok(());
        }
    };

    match user_service.create_or_find_user(user_id, username).await {
        Ok(_) => {}
        Err(e) => {
            bot.send_message(message.chat.id, e.to_string()).await?;
            tracing::error!("Failed to create or find user: {}", e);
            return Err(e.into());
        }
    }

    Ok(())
}

#[tracing::instrument(skip(dialog))]
pub async fn handle_subscribe_command(message: Message, bot: Bot, dialog: BotDialog) -> anyhow::Result<()> {
    dialog.update(State::SubscribeWaitingTargetChat).await?;
    bot.send_message(message.chat.id, "Enter the chat ID where you want to receive updates").await?;
    Ok(())
}

#[tracing::instrument(skip(dialog))]
pub async fn handle_subscribe_enter_chat_id(message: Message, bot: Bot, dialog: BotDialog) -> anyhow::Result<()> {
    let target_chat = message.text().and_then(|text| text.parse::<i64>().ok()).unwrap_or(message.chat.id.0);
    dialog.update(State::SubscribeWaitingUrl { target_chat }).await?;
    bot.send_message(message.chat.id, "Enter the URL of the RSS feed you want to subscribe to").await?;
    Ok(())
}

#[tracing::instrument(skip(dialog))]
pub async fn handle_subscribe_enter_url(message: Message, bot: Bot, dialog: BotDialog, service: Arc<subscription::Service>) -> anyhow::Result<()> {
    let target_chat = match dialog.get().await {
        Ok(Some(State::SubscribeWaitingUrl { target_chat })) => target_chat,
        _ => {
            bot.send_message(message.chat.id, "Invalid state").await?;
            dialog.reset().await?;
            return Err(anyhow!("Invalid state"));
        }
    };

    let user_id = message.from().map(|user| user.id.0 as i64);
    if user_id.is_none() {
        bot.send_message(message.chat.id, "User ID not found").await?;
        return Ok(());
    }

    let url = match message.text() {
        Some(url) => match url.parse() {
            Ok(url) => url,
            Err(_) => {
                bot.send_message(message.chat.id, "Invalid URL").await?;
                return Ok(());
            }
        },
        None => {
            bot.send_message(message.chat.id, "You didn't provide a URL").await?;
            return Ok(());
        }
    };

    match service.add_subscription(user_id.unwrap(), target_chat, url).await {
        Ok(_) => bot.send_message(message.chat.id, "Subscription added.").await?,
        Err(e) => {
            bot.send_message(message.chat.id, e.to_string()).await?;
            return Err(e.into());
        }
    };

    dialog.reset().await?;

    Ok(())
}

#[tracing::instrument(skip(dialog))]
pub async fn handle_unsubscribe_command(message: Message, bot: Bot, dialog: BotDialog, service: Arc<subscription::Service>) -> anyhow::Result<()> {
    let user_id = message.from().map(|user| user.id.0 as i64);
    if user_id.is_none() {
        bot.send_message(message.chat.id, "User ID not found").await?;
        return Ok(());
    }

    let subscriptions = service.list_subscriptions(user_id.unwrap()).await?;
    if subscriptions.is_empty() {
        bot.send_message(message.chat.id, "You have no subscriptions").await?;
        return Ok(());
    }

    let mut buttons = subscriptions.iter()
        .map(|sub| {
            InlineKeyboardButton::new(format!(
                "{} -> {}",
                sub.url,
                sub.target_chat
            ), InlineKeyboardButtonKind::CallbackData(sub.id.to_string()))
        })
        .collect::<Vec<_>>();

    buttons.push(InlineKeyboardButton::new("Cancel", InlineKeyboardButtonKind::CallbackData("cancel".to_string())));

    bot
        .send_message(message.chat.id, "Select a subscription to unsubscribe from")
        .reply_markup(InlineKeyboard(InlineKeyboardMarkup::new(
            buttons.chunks(1)
                .map(|row| row.to_vec())
                .collect::<Vec<_>>()
        )))
        .await?;

    dialog.update(State::UnsubscribeWaitingCallbackQuery).await?;

    Ok(())
}

#[tracing::instrument(skip(dialog))]
pub async fn handle_unsubscribe_callback(query: CallbackQuery, bot: Bot, service: Arc<subscription::Service>, dialog: BotDialog) -> anyhow::Result<()> {
    let data = query.data.unwrap_or_default();
    if data == "cancel" {
        bot.answer_callback_query(query.id).text("Cancelled").send().await?;
        dialog.reset().await?;
        return Ok(());
    }

    let user_id = query.from.id.0 as i64;
    let subscription_id = match data.parse::<i32>() {
        Ok(id) => id,
        Err(_) => {
            bot.answer_callback_query(query.id).text("Invalid subscription ID").send().await?;
            return Ok(());
        }
    };

    match service.remove_subscription(user_id, subscription_id).await {
        Ok(_) => {
            bot.answer_callback_query(query.id).text("Subscription removed").send().await?;
        }
        Err(e) => {
            bot.answer_callback_query(query.id).text(e.to_string()).send().await?;
            return Err(e.into());
        }
    }

    Ok(())
}

#[tracing::instrument(skip(dialog))]
pub async fn handle_list_command(message: Message, bot: Bot, dialog: BotDialog, service: Arc<subscription::Service>) -> anyhow::Result<()> {
    let user_id = message.from().map(|user| user.id.0 as i64);
    if user_id.is_none() {
        bot.send_message(message.chat.id, "User ID not found").await?;
        return Ok(());
    }

    let subscriptions = service.list_subscriptions(user_id.unwrap()).await?;
    if subscriptions.is_empty() {
        bot.send_message(message.chat.id, "You have no subscriptions").await?;
        return Ok(());
    }

    let content = subscriptions.iter()
        .fold("*Your subscriptions:*\n".to_string(), |acc, sub| {
            format!("{}\nID {}: {} -> `{}`", acc, sub.id, sub.url, sub.target_chat)
        });

    bot.send_message(message.chat.id, content).parse_mode(ParseMode::Markdown).await?;

    dialog.reset().await?;
    Ok(())
}
