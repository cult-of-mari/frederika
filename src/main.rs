use anyhow::Result;
use std::path::PathBuf;
use teloxide::prelude::*;

mod config;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let config_path = PathBuf::from("./Config.toml");
    let config = config::load_config(&config_path)?;

    log::info!("Starting the bot...");
    let bot = Bot::new(config.telegram.token);

    teloxide::repl(bot, |bot: Bot, msg: Message| async move {
        if let Some(text) = msg.text() {
            log::debug!("Recieved message: {text}");
            let me = bot.get_me().await?;
            log::debug!("{me:?}");
            let is_mention = text.contains(me.username());
            log::debug!("Is mention: {is_mention}");
            if is_mention {
                bot.send_dice(msg.chat.id).await?;
                bot.send_message(msg.chat.id, "Nipah ^_^").await?;
            }
        };

        Ok(())
    })
    .await;

    Ok(())
}
