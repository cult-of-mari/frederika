use anyhow::Result;
use google_gemini;
use std::path::PathBuf;
use teloxide::{
    dispatching::{dialogue::InMemStorage, UpdateFilterExt},
    prelude::*,
};

mod config;
mod msg_cache;
use msg_cache::MessageCache;

async fn message_handler(
    bot: Bot,
    dialogue: Dialogue<MessageCache, InMemStorage<MessageCache>>,
    mut msg_cache: MessageCache,
    msg: Message,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if let Some(text) = msg.text() {
        log::debug!("{msg_cache:?}");
        log::debug!("Recieved message: {text}");
        let me = bot.get_me().await?;
        log::debug!("{me:?}");
        let is_mention = text.contains(me.username());
        log::debug!("Is mention: {is_mention}");
        if is_mention {
            bot.send_dice(msg.chat.id).await?;
            bot.send_message(msg.chat.id, "Nipah ^_^").await?;
        }
        msg_cache.add(msg);
    };
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let config_path = PathBuf::from("./Config.toml");
    let config = config::load_config(&config_path)?;

    log::info!("Starting the bot...");
    let bot = Bot::new(config.telegram.token);

    let handler = Update::filter_message()
        .enter_dialogue::<Message, InMemStorage<MessageCache>, MessageCache>()
        .branch(dptree::endpoint(message_handler));

    Dispatcher::builder(bot, handler)
        .dependencies(dptree::deps![InMemStorage::<MessageCache>::new()])
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;

    Ok(())
}
