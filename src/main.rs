use anyhow::Result;
use google_gemini;
use google_gemini::{
    GeminiClient, GeminiMessage, GeminiPart, GeminiRequest, GeminiRole, GeminiSafetySetting,
    GeminiSafetyThreshold, GeminiSystemPart,
};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
use teloxide::dispatching::dialogue::GetChatId;
use teloxide::dispatching::UpdateFilterExt;
use teloxide::prelude::*;

mod config;
mod msg_cache;

use config::Config;
use msg_cache::MessageCache;

struct BotState {
    config: Config,
    gemini: GeminiClient,
    msg_cache: Mutex<MessageCache>,
}

impl BotState {
    fn new(config: Config) -> Self {
        let gemini_token = config.gemini.token.clone();
        let cache_size = config.telegram.cache_size;
        Self {
            config,
            gemini: GeminiClient::new(gemini_token),
            msg_cache: Mutex::new(MessageCache::new(cache_size)),
        }
    }
}

async fn handle_message(
    bot: Bot,
    msg: Message,
    state: Arc<BotState>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if let Some(text) = msg.text() {
        log::debug!("Recieved message: {text}");
        log::debug!("ChatId: {}", msg.chat.id);
        let me = bot.get_me().await?;
        log::debug!("{me:?}");
        let is_mention = text.contains(me.username());
        log::debug!("Is mention: {is_mention}");
        if is_mention {
            bot.send_dice(msg.chat.id).await?;
            bot.send_message(msg.chat.id, "Nipah ^_^").await?;
        }
        let mut msg_cache = state.msg_cache.lock().unwrap();
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
    let bot = Bot::new(config.telegram.token.clone());
    let state = Arc::new(BotState::new(config));

    let handler = Update::filter_message().branch(dptree::endpoint(handle_message));
    Dispatcher::builder(bot, handler)
        .dependencies(dptree::deps![state])
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;

    Ok(())
}
