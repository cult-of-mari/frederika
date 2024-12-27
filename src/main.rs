use anyhow::Result;
use figment::Result;
use serde::Deserialize;
use std::path::Path; use teloxide::prelude::*;

#[derive(Debug, Deserialize)]
struct Config {
    token: String,

}

pub fn load_config(config_path: &Path) -> Result<Config> {
    Figment::new().merge(Toml::file("config.toml")).extract()
}

#[tokio::main]
async fn main() -> Result<()> {
    pretty_env_logger::init();
    log::info!("Starting throw dice bot...");

    let bot = Bot::from_env();

    teloxide::repl(bot, |bot: Bot, msg: Message| async move {
        bot.send_dice(msg.chat.id).await?;
        Ok(())
    })
    .await;

    Ok(())
}
