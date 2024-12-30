use aho_corasick::AhoCorasick;
use anyhow::Result;
use google_gemini::{
    GeminiClient, GeminiMessage, GeminiPart, GeminiRequest, GeminiRole, GeminiSafetySetting,
    GeminiSafetyThreshold, GeminiSystemPart,
};
use serde::Serialize;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
use teloxide::{
    dispatching::UpdateFilterExt,
    prelude::*,
    types::{Me, MessageId, ParseMode},
};

mod config;
mod msg_cache;

use config::Config;
use msg_cache::MessageCache;

#[derive(Serialize)]
struct MessageInfo<'a> {
    user_name: String,
    user_id: UserId,
    message_content: &'a str,
    message_id: MessageId,
}

struct BotState {
    me: Me,
    config: Config,
    gemini: GeminiClient,
    msg_cache: Mutex<MessageCache>,
    name_matcher: AhoCorasick,
}

impl BotState {
    fn new(config: Config, me: Me) -> Self {
        let gemini_token = config.gemini.token.clone();
        let cache_size = config.telegram.cache_size;
        let mut names = config.telegram.names.clone();
        names.push(me.username().to_string());
        Self {
            me,
            config,
            gemini: GeminiClient::new(gemini_token),
            msg_cache: Mutex::new(MessageCache::new(cache_size)),
            name_matcher: AhoCorasick::builder()
                .ascii_case_insensitive(true)
                .build(names)
                .unwrap(),
        }
    }

    fn should_reply(&self, msg: &Message) -> bool {
        msg.reply_to_message()
            .and_then(|msg| msg.from.clone())
            .map(|user| user.eq(&self.me))
            .unwrap_or(false)
            || self.name_matcher.is_match(msg.text().unwrap())
    }

    async fn get_gemini_reply(&self, msg: &Message) -> String {
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

        let message_history = self.build_message_history(msg);
        message_history.iter().cloned().for_each(|(role, content)| {
            let parts = vec![GeminiPart::from(content)];
            request.contents.push(GeminiMessage::new(role, parts));
        });

        let reply_text = match self.gemini.generate(request).await {
            Ok(content) => {
                log::debug!("Response content: {content}");
                content
            }
            Err(error) => format!("```\n{error}\n```\nReport this issue to the admins"),
        };
        BotState::sanitize_text(reply_text.as_str())
    }

    fn build_message_history(&self, last_msg: &Message) -> Vec<(GeminiRole, String)> {
        self.msg_cache
            .lock()
            .unwrap()
            .messages(last_msg.chat.id)
            .chain([last_msg])
            .filter_map(|msg| {
                msg.from
                    .clone()
                    .map(|user| MessageInfo {
                        user_name: user.full_name(),
                        user_id: user.id,
                        message_content: msg.text().unwrap_or_default(),
                        message_id: msg.id,
                    })
                    .map(|info| {
                        (
                            if info.user_id.eq(&self.me.id) {
                                GeminiRole::Model
                            } else {
                                GeminiRole::User
                            },
                            serde_json::to_string(&info).unwrap(),
                        )
                    })
            })
            .collect()
    }

    fn sanitize_text(s: &str) -> String {
        vec!["<p>", "</p>", "<br />", "<li>", "</li>", "<ol>", "</ol>"]
            .iter()
            .fold(markdown::to_html(s), |s, pattern| s.replace(pattern, ""))
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
        if state.should_reply(&msg) {
            let reply = state.get_gemini_reply(&msg).await;
            log::debug!("Reply: {reply}");
            if let Err(error) = bot
                .send_message(msg.chat.id, reply)
                .parse_mode(ParseMode::Html)
                .await
            {
                log::error!("Failed to send message: {error}");
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
    let me = bot.get_me().await?;
    log::debug!("{me:?}");
    let state = Arc::new(BotState::new(config, me));

    let handler = Update::filter_message().branch(dptree::endpoint(handle_message));
    Dispatcher::builder(bot, handler)
        .dependencies(dptree::deps![state])
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;

    Ok(())
}
