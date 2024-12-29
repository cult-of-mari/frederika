use aho_corasick::AhoCorasick;
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
use teloxide::{
    dispatching::UpdateFilterExt,
    prelude::*,
    types::{ParseMode, User},
};

mod config;
mod msg_cache;

use config::Config;
use msg_cache::MessageCache;

struct BotState {
    config: Config,
    gemini: GeminiClient,
    msg_cache: Mutex<MessageCache>,
    name_matcher: AhoCorasick,
}

impl BotState {
    fn new(config: Config) -> Self {
        let gemini_token = config.gemini.token.clone();
        let cache_size = config.telegram.cache_size;
        let names = config.telegram.names.clone();
        Self {
            config,
            gemini: GeminiClient::new(gemini_token),
            msg_cache: Mutex::new(MessageCache::new(cache_size)),
            name_matcher: AhoCorasick::builder()
                .ascii_case_insensitive(true)
                .build(names)
                .unwrap(),
        }
    }

    fn should_reply(&self, me: &User, msg: &Message) -> bool {
        msg.reply_to_message()
            .and_then(|msg| msg.from.clone())
            .map(|user| user.eq(me))
            .unwrap_or(false)
            || self.name_matcher.is_match(msg.text().unwrap())
    }

    async fn get_gemini_reply(&self, msg: &Message) -> String {
        let msg_text = msg.text().unwrap();

        let mut request = GeminiRequest::default();

        request.system_instruction.parts.push(GeminiSystemPart {
            text: self.config.gemini.personality.clone(),
        });

        let settings = [
            GeminiSafetySetting::HarmCategoryHarassment,
            GeminiSafetySetting::HarmCategoryHateSpeech,
            GeminiSafetySetting::HarmCategorySexuallyExplicit,
            GeminiSafetySetting::HarmCategoryDangerousContent,
            GeminiSafetySetting::HarmCategoryCivicIntegrity,
        ];

        let settings = settings.map(|setting| (setting)(GeminiSafetyThreshold::BlockNone));

        request.safety_settings.extend(settings);

        let parts = vec![GeminiPart::from(msg_text.to_string())];
        request
            .contents
            .push(GeminiMessage::new(GeminiRole::User, parts));

        let reply_text = match self.gemini.generate(request).await {
            Ok(content) => {
                log::debug!("Response content: {content}");
                content
            }
            Err(error) => format!("```\n{error}\n```\nreport this issue to the admins"),
        };
        BotState::santize_text(reply_text.as_str())
    }

    fn santize_text(s: &str) -> String {
        markdown::to_html(s).replace("<p>", "").replace("</p>", "")
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
        if state.should_reply(&me, &msg) {
            let reply = state.get_gemini_reply(&msg).await;
            log::debug!("Reply: {reply}");
            if let Err(error) = bot
                .send_message(msg.chat.id, reply)
                .parse_mode(ParseMode::Html)
                .await
            {
                log::error!("failed to send message: {error}");
            }
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
