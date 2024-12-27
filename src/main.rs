use anyhow::Result;
use std::path::PathBuf;
use teloxide::prelude::*;

mod config;

#[tokio::main]
async fn main() -> Result<()> {
    let config_path = PathBuf::from("./Config.toml");
    let config = config::load_config(&config_path)?;

    log::info!("Starting throw dice bot...");
    let bot = Bot::new(config.telegram.token);

    teloxide::repl(bot, |bot: Bot, msg: Message| async move {
        let me = bot.get_me().await?;
        let is_mention = msg
            .mentioned_users()
            .filter(|user| user.id == me.id)
            .count()
            == 1;
        if is_mention {
            bot.send_dice(msg.chat.id).await?;
        }
        Ok(())
    })
    .await;

    Ok(())
}
