use sea_orm::Database;
use teloxide::dispatching::dialogue::{RedisStorage, serializer};
use teloxide::prelude::*;
use teloxide::update_listeners::webhooks::Options;

use crate::handlers::{State, UnstatedCommand};

mod handlers;

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

    let bot = Bot::new(&config.bot_token);

    let handlers = dptree::entry().branch(
        Update::filter_message()
            .enter_dialogue::<Message, RedisStorage<serializer::Json>, State>()
            .branch(
                dptree::case![State::Unstated]
                    .filter_command::<UnstatedCommand>()
                    .branch(
                        dptree::case![UnstatedCommand::Help].endpoint(handlers::handle_help),
                    )
            )
    );

    let listener = teloxide::update_listeners::webhooks::axum(
        bot.clone(),
        Options::new(
            config.webhook_address.parse()?,
            config.webhook_url.parse()?,
        ),
    ).await?;


    Dispatcher::builder(bot, handlers)
        .distribution_function(|_| None::<std::convert::Infallible>)
        .dependencies(dptree::deps![])
        .build()
        .dispatch_with_listener(listener, LoggingErrorHandler::new())
        .await;

    db.close().await?;
    
    Ok(())
}
