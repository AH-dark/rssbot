use std::sync::Arc;

use distributed_scheduler::cron::Cron;
use distributed_scheduler::driver::redis_zset::RedisZSetDriver;
use distributed_scheduler::node_pool::NodePool;
use sea_orm::Database;
use teloxide::dispatching::dialogue::{RedisStorage, serializer};
use teloxide::prelude::*;
use teloxide::update_listeners::webhooks::Options;

mod handlers;
mod services;
mod filters;
mod data;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv::dotenv().ok();

    let config = rssbot_common::config::Config::new()?;

    // Initialize the tracer
    rssbot_common::observability::tracing::init_tracer(
        env!("CARGO_PKG_NAME").to_string(),
        env!("CARGO_PKG_VERSION").to_string(),
        &config,
    );

    pretty_env_logger::try_init().ok();

    let db = Database::connect(&config.database_url).await?;
    let redis_client = redis::Client::open(config.redis_url.as_str())?;

    let scheduler = {
        let node_id = uuid::Uuid::new_v4().to_string();
        let driver = RedisZSetDriver::new(redis_client.clone(), env!("CARGO_PKG_NAME"), node_id.as_str()).await?;
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
    let redis_con = redis_client.get_multiplexed_tokio_connection().await?;

    let private_message_handlers = dptree::entry()
        .filter(filters::private_message_only)
        .branch(
            Update::filter_message()
                .enter_dialogue::<Message, RedisStorage<serializer::Json>, handlers::private::State>()
                .branch(
                    dptree::case![handlers::private::State::Unstated]
                        .filter_command::<handlers::private::UnstatedCommand>()
                        .branch(dptree::case![handlers::private::UnstatedCommand::Start].endpoint(handlers::private::handle_start))
                        .branch(dptree::case![handlers::private::UnstatedCommand::Help].endpoint(handlers::private::handle_help))
                        .branch(dptree::case![handlers::private::UnstatedCommand::Subscribe].endpoint(handlers::private::handle_subscribe_command))
                        .branch(dptree::case![handlers::private::UnstatedCommand::List].endpoint(handlers::private::handle_list_command))
                        .branch(dptree::case![handlers::private::UnstatedCommand::Unsubscribe].endpoint(handlers::private::handle_unsubscribe_command))
                )
                .branch(dptree::case![handlers::private::State::SubscribeWaitingUrl].endpoint(handlers::private::handle_subscribe_enter_url))
        )
        .branch(
            Update::filter_callback_query()
                .enter_dialogue::<CallbackQuery, RedisStorage<serializer::Json>, handlers::private::State>()
                .branch(dptree::case![handlers::private::State::UnsubscribeWaitingCallbackQuery].endpoint(handlers::private::handle_unsubscribe_callback))
        );

    let channel_or_group_handlers = dptree::entry()
        .filter(filters::channel_or_group)
        .branch(
            Update::filter_message()
                .filter_command::<handlers::public::Command>()
                .branch(dptree::case![handlers::public::Command::Start { id }].endpoint(handlers::public::handle_start))
                .branch(dptree::case![handlers::public::Command::Help].endpoint(handlers::public::handle_unstated_help))
        );

    let mut dispatcher = Dispatcher::builder(
        bot,
        dptree::entry()
            .branch(channel_or_group_handlers)
            .branch(private_message_handlers),
    )
        .distribution_function(|_| None::<std::convert::Infallible>)
        .dependencies(dptree::deps![state_storage, subscription_service, user_service, redis_con])
        .build();

    tokio::select! {
        _ = scheduler.start() => {}
        _ = dispatcher.dispatch_with_listener(listener, LoggingErrorHandler::new()) => {}
    }

    db.close().await?;

    Ok(())
}
