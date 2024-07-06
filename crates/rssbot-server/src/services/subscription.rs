use std::str::FromStr;

use chrono::NaiveDateTime;
use sea_orm::ActiveValue;
use sea_orm::prelude::*;
use teloxide::prelude::*;

use rssbot_entities::subscription;

#[derive(Debug, Clone)]
pub struct Service {
    db: DatabaseConnection,
    bot: Bot,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Database error: {0}")]
    DatabaseError(#[from] DbErr),
}

impl Service {
    pub fn new(db: DatabaseConnection, bot: Bot) -> Self {
        Self { db, bot }
    }

    #[tracing::instrument]
    pub async fn add_subscription(&self, user_id: i64, target_chat: i64, url: String) -> Result<subscription::Model, Error> {
        let subscription = subscription::ActiveModel {
            user_refer: ActiveValue::Set(user_id),
            target_chat: ActiveValue::Set(target_chat),
            url: ActiveValue::Set(url),
            last_updated: ActiveValue::Set(None),
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
    pub async fn sync_subscriptions(&self) -> Result<(), Error> {
        log::info!("Syncing subscriptions");

        let subscriptions = subscription::Entity::find().all(&self.db).await?;

        let client = reqwest::Client::new();
        for subscription in subscriptions {
            let feed = match Self::get_feed(&client, &subscription).await {
                Ok(feed) => feed,
                Err(err) => {
                    tracing::error!("Failed to fetch feed: {:?}", err);
                    continue;
                }
            };

            tracing::debug!("Fetched feed: {:?}", feed);

            let new_items = feed.items()
                .iter()
                .filter(|item| {
                    let item_date = match item.pub_date() {
                        Some(date) => match NaiveDateTime::parse_from_str(date, "%a, %d %b %Y %H:%M:%S %z") {
                            Ok(date) => date,
                            Err(err) => {
                                tracing::warn!("Failed to parse date: {}", err);
                                return false;
                            }
                        },
                        None => {
                            tracing::warn!("Item has no date: {:?}", item);
                            return false;
                        }
                    };

                    let last_updated = match subscription.last_updated {
                        Some(date) => date,
                        None => {
                            tracing::warn!("Subscription has no last updated date: {:?}", subscription);
                            return false;
                        }
                    };

                    item_date > last_updated
                })
                .collect::<Vec<_>>();

            log::info!("Subscription {} has {} updates, fetched on {}", subscription.id, new_items.len(), feed.pub_date().unwrap_or_default());

            for item in new_items {
                let (title, description, link) = match (item.title(), item.description(), item.link()) {
                    (Some(title), Some(description), Some(link)) => (title, description, link),
                    _ => {
                        tracing::warn!("Item is missing title, description, or link: {:?}", item);
                        continue;
                    }
                };

                let link = match link.parse() {
                    Ok(link) => link,
                    Err(err) => {
                        tracing::warn!("Failed to parse link: {}", err);
                        continue;
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
            }
        }

        Ok(())
    }

    #[tracing::instrument]
    async fn get_feed(client: &reqwest::Client, subscription: &subscription::Model) -> Result<rss::Channel, SubscriptionError> {
        let url = subscription.url.clone();
        let response = client
            .get(&url)
            .timeout(std::time::Duration::from_secs(10))
            .header(reqwest::header::USER_AGENT, format!("rssbot/{}", env!("CARGO_PKG_VERSION")))
            .send()
            .await?;

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
enum SubscriptionError {
    #[error("Failed to fetch feed: {0}")]
    FetchError(#[from] reqwest::Error),
    #[error("Failed to parse feed: {0}")]
    ParseError(#[from] rss::Error),
    #[error("Response status is not OK: {0}")]
    ResponseStatusNotOk(reqwest::StatusCode),
}
