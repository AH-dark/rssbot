use std::str::FromStr;

use chrono::NaiveDateTime;
use sea_orm::ActiveValue;
use sea_orm::prelude::*;
use teloxide::prelude::*;

use rssbot_common::chrono_utils;
use rssbot_entities::subscription;

#[derive(Debug, Clone)]
pub struct Service {
    db: DatabaseConnection,
    bot: Bot,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Subscription already exists")]
    SubscriptionAlreadyExists,
    #[error("Subscription created by other user")]
    SubscriptionCreatedByOtherUser,
    #[error("Database error: {0}")]
    Database(#[from] DbErr),
    #[error("RSS error: {0}")]
    Rss(#[from] SubscriptionError),
}

impl Service {
    pub fn new(db: DatabaseConnection, bot: Bot) -> Self {
        Self { db, bot }
    }

    #[tracing::instrument]
    pub async fn add_subscription(&self, user_id: i64, target_chat: i64, url: String) -> Result<subscription::Model, Error> {
        let existing = subscription::Entity::find()
            .filter(subscription::Column::UserRefer.eq(user_id))
            .filter(subscription::Column::Url.eq(&url))
            .one(&self.db)
            .await?;

        // check repeat subscription
        if let Some(subscription) = existing {
            return if subscription.user_refer != user_id {
                Err(Error::SubscriptionCreatedByOtherUser)
            } else {
                Err(Error::SubscriptionAlreadyExists)
            };
        }

        let subscription = subscription::ActiveModel {
            user_refer: ActiveValue::Set(user_id),
            target_chat: ActiveValue::Set(target_chat),
            url: ActiveValue::Set(url),
            last_updated: ActiveValue::Set(chrono::Utc::now().naive_utc()),
            last_sent: ActiveValue::Set(None),
            last_error: ActiveValue::Set(None),
            ..Default::default()
        }
            .insert(&self.db)
            .await?;

        tracing::debug!("Subscription added: {:?}", subscription);

        Ok(subscription)
    }

    #[tracing::instrument]
    pub async fn remove_subscription(&self, user_id: i64, id: i32) -> Result<(), Error> {
        subscription::ActiveModel {
            id: ActiveValue::Set(id),
            user_refer: ActiveValue::Set(user_id),
            ..Default::default()
        }
            .delete(&self.db)
            .await?;

        Ok(())
    }

    #[tracing::instrument]
    pub async fn list_subscriptions(&self, user_id: i64) -> Result<Vec<subscription::Model>, Error> {
        let subscriptions = subscription::Entity::find()
            .filter(subscription::Column::UserRefer.eq(user_id))
            .all(&self.db)
            .await?;

        Ok(subscriptions)
    }

    #[tracing::instrument]
    pub async fn list_subscriptions_for_chat(&self, chat_id: i64) -> Result<Vec<subscription::Model>, Error> {
        let subscriptions = subscription::Entity::find()
            .filter(subscription::Column::TargetChat.eq(chat_id))
            .all(&self.db)
            .await?;

        Ok(subscriptions)
    }

    #[tracing::instrument]
    pub async fn sync_subscriptions(&self) -> Result<(), Error> {
        log::info!("Syncing subscriptions");

        let subscriptions = subscription::Entity::find().all(&self.db).await?;

        for subscription in subscriptions {
            match self.sync_single_subscription(&subscription).await {
                Ok((pub_date, len)) => {
                    log::info!("Subscription {} synced, {} updates.", subscription.id, len);

                    let mut act: subscription::ActiveModel = subscription.into();
                    act.last_updated = ActiveValue::Set(chrono::Utc::now().naive_utc());
                    act.last_sent = ActiveValue::Set(pub_date);
                    act.last_error = ActiveValue::Set(None);

                    act.update(&self.db).await?;
                }
                Err(err) => {
                    log::error!("Failed to sync subscription: {}", err);

                    let mut act: subscription::ActiveModel = subscription.into();
                    act.last_updated = ActiveValue::Set(chrono::Utc::now().naive_utc());
                    act.last_sent = ActiveValue::Set(None);
                    act.last_error = ActiveValue::Set(Some(err.to_string()));
                    act.update(&self.db).await?;

                    continue;
                }
            };
        }

        Ok(())
    }

    async fn sync_single_subscription(&self, subscription: &subscription::Model) -> Result<(Option<NaiveDateTime>, usize), Error> {
        let feed = match Self::get_feed(subscription).await {
            Ok(feed) => feed,
            Err(err) => {
                tracing::error!("Failed to fetch feed: {:?}", err);
                return Err(err.into());
            }
        };

        tracing::debug!("Fetched feed: {:?}", feed);

        let new_items = feed.items()
            .iter()
            .filter(|item| {
                let item_date = match item.pub_date().and_then(chrono_utils::parse_datetime) {
                    Some(date) => date,
                    None => {
                        tracing::warn!("Date format is not recognized: {:?}", item.pub_date());
                        return false;
                    }
                };

                item_date > subscription.last_updated
            })
            .collect::<Vec<_>>();

        log::info!("Subscription {} has {} updates, fetched on {}", subscription.id, new_items.len(), feed.pub_date().unwrap_or_default());

        if new_items.is_empty() {
            return Ok((None, 0));
        }

        let len = new_items.len();
        for item in new_items {
            if let Err(err) = self.handle_new_item(subscription, item).await {
                log::error!("Failed to handle new item: {}", err);
            }
        }

        Ok((
            feed
                .items()
                .iter()
                .filter_map(|item| item.pub_date())
                .filter_map(|date| NaiveDateTime::parse_from_str(date, "%a, %d %b %Y %H:%M:%S %z").ok())
                .max(),
            len
        ))
    }

    async fn handle_new_item(&self, subscription: &subscription::Model, item: &rss::Item) -> Result<(), Error> {
        let (title, description, link) = match (item.title(), item.description(), item.link()) {
            (Some(title), Some(description), Some(link)) => (title, description, link),
            _ => {
                tracing::warn!("Item is missing title, description, or link: {:?}", item);
                return Ok(());
            }
        };

        let link = match link.parse() {
            Ok(link) => link,
            Err(err) => {
                tracing::warn!("Failed to parse link: {}", err);
                return Ok(());
            }
        };

        let message = format!(
            "📰 *{}*\n\n{}",
            title,
            description,
        );

        // notify the user
        match self.bot.send_message(ChatId(subscription.target_chat), message)
            .reply_markup(teloxide::types::ReplyMarkup::InlineKeyboard(teloxide::types::InlineKeyboardMarkup::new(vec![
                vec![
                    teloxide::types::InlineKeyboardButton::url("Read more", link),
                ],
            ])))
            .parse_mode(teloxide::types::ParseMode::MarkdownV2)
            .send()
            .await {
            Ok(_) => {
                tracing::debug!("Sent message for item: {}", item.title().unwrap_or_default());
            }
            Err(err) => {
                log::error!("Failed to send message for item: {}", err);
            }
        }

        Ok(())
    }

    #[tracing::instrument]
    async fn get_feed(subscription: &subscription::Model) -> Result<rss::Channel, SubscriptionError> {
        let response = match reqwest::get(&subscription.url).await {
            Ok(response) => response,
            Err(err) => {
                return Err(err.into());
            }
        };

        let status = response.status();
        if !status.is_success() {
            return Err(SubscriptionError::ResponseStatusNotOk(status));
        }

        let body = response.text().await?;
        let feed = rss::Channel::from_str(&body)?;

        Ok(feed)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SubscriptionError {
    #[error("Failed to fetch feed: {0}")]
    FetchError(#[from] reqwest::Error),
    #[error("Failed to parse feed: {0}")]
    ParseError(#[from] rss::Error),
    #[error("Response status is not OK: {0}")]
    ResponseStatusNotOk(reqwest::StatusCode),
}
