use std::sync::Arc;

use distributed_scheduler::cron::Cron;
use distributed_scheduler::driver::redis_zset::RedisZSetDriver;
use distributed_scheduler::node_pool::NodePool;
use sea_orm::Database;
use teloxide::dispatching::dialogue::{RedisStorage, serializer};
use teloxide::prelude::*;
use teloxide::update_listeners::webhooks::Options;

use crate::handlers::{State, UnstatedCommand};

mod handlers;
mod services;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv::dotenv().ok();
    pretty_env_logger::init();

    let config = rssbot_common::config::Config::new()?;

    // Initialize the tracer
    rssbot_common::observability::tracing::init_tracer(
        env!("CARGO_PKG_NAME").to_string(),
        env!("CARGO_PKG_VERSION").to_string(),
        &config,
    );

    let db = Database::connect(&config.database_url).await?;
    let redis_client = redis::Client::open(config.redis_url.as_str())?;

    let scheduler = {
        let node_id = uuid::Uuid::new_v4().to_string();
        let driver = RedisZSetDriver::new(redis_client, env!("CARGO_PKG_NAME"), node_id.as_str()).await?;
        let node_pool = NodePool::new(driver).await?;
        Cron::new(node_pool).await
    };

    let bot = Bot::new(&config.bot_token)
        .set_api_url(config.api_server.parse()?);

    let subscription_service = Arc::new(services::subscription::Service::new(db.clone(), bot.clone()));
    let user_service = Arc::new(services::user::Service::new(db.clone()));

    scheduler.add_async_job(
        "sync_subscription",
        "0 * * * * *".parse()?,
        {
            let subscription_service = subscription_service.clone();
            move || {
                let service = subscription_service.clone();
                async move {
                    service.sync_subscriptions().await?;
                    Ok(())
                }
            }
        },
    ).await?;

    let state_storage = RedisStorage::open(config.redis_url.as_str(), serializer::Json).await?;
    let listener = teloxide::update_listeners::webhooks::axum(
        bot.clone(),
        Options::new(
            config.webhook_address.parse()?,
            config.webhook_url.parse()?,
        ),
    ).await?;
    let handlers = dptree::entry()
        .branch(
            Update::filter_message()
                .enter_dialogue::<Message, RedisStorage<serializer::Json>, State>()
                .branch(
                    dptree::case![State::Unstated]
                        .filter_command::<UnstatedCommand>()
                        .branch(dptree::case![UnstatedCommand::Start].endpoint(handlers::handle_start))
                        .branch(dptree::case![UnstatedCommand::Help].endpoint(handlers::handle_help))
                        .branch(dptree::case![UnstatedCommand::Subscribe].endpoint(handlers::handle_subscribe_command))
                        .branch(dptree::case![UnstatedCommand::List].endpoint(handlers::handle_list_command))
                        .branch(dptree::case![UnstatedCommand::Unsubscribe].endpoint(handlers::handle_unsubscribe_command))
                )
                .branch(
                    dptree::case![State::SubscribeWaitingTargetChat].endpoint(handlers::handle_subscribe_enter_chat_id)
                )
                .branch(
                    dptree::case![State::SubscribeWaitingUrl { target_chat }].endpoint(handlers::handle_subscribe_enter_url)
                )
        )
        .branch(
            Update::filter_callback_query()
                .enter_dialogue::<CallbackQuery, RedisStorage<serializer::Json>, State>()
                .branch(
                    dptree::case![State::UnsubscribeWaitingCallbackQuery].endpoint(handlers::handle_unsubscribe_callback)
                )
        );

    let mut dispatcher = Dispatcher::builder(bot, handlers)
        .distribution_function(|_| None::<std::convert::Infallible>)
        .dependencies(dptree::deps![state_storage, subscription_service, user_service])
        .build();

    tokio::select! {
        _ = scheduler.start() => {}
        _ = dispatcher.dispatch_with_listener(listener, LoggingErrorHandler::new()) => {}
    }

    db.close().await?;

    Ok(())
}
