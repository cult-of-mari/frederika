use aho_corasick::AhoCorasick;
use anyhow::{anyhow, Result};
use futures_util::StreamExt;
use google_gemini::{
    GeminiClient, GeminiMessage, GeminiPart, GeminiRequest, GeminiRole, GeminiSafetySetting,
    GeminiSafetyThreshold, GeminiSystemPart,
};
use serde::Serialize;
use std::sync::Arc;
use teloxide::{
    dispatching::UpdateFilterExt,
    prelude::*,
    types::{ChatKind, Me, MediaKind, MessageId, MessageKind, ParseMode},
};
use tokio::sync::Mutex;

mod cli;
mod config;
mod msg_cache;

use cli::parse_cli;
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
        matches!(msg.kind, MessageKind::Common(_))
            && (matches!(msg.chat.kind, ChatKind::Private(_))
                || msg
                    .reply_to_message()
                    .and_then(|msg| msg.from.as_ref())
                    .map(|user| user.eq(&self.me))
                    .unwrap_or(false)
                || self.name_matcher.is_match(msg.text().unwrap_or_default()))
    }

    async fn get_gemini_reply(&self, bot: &Bot, msg: &Message) -> String {
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

        self.build_message_history(bot, msg)
            .await
            .into_iter()
            .for_each(|msg| request.contents.push(msg));
        let reply_text = match self.gemini.generate(request).await {
            Ok(content) => {
                log::debug!("Response content: {content}");
                content
            }
            Err(error) => format!("```\n{error}\n```\nReport this issue to the admins"),
        };
        BotState::sanitize_text(reply_text.as_str())
    }

    async fn build_message_history(&self, bot: &Bot, last_msg: &Message) -> Vec<GeminiMessage> {
        let mut msg_cache = self.msg_cache.lock().await;
        let futures = msg_cache
            .messages(last_msg.chat.id)
            .chain([last_msg])
            .map(|msg| self.message_into_gemini_message(bot, msg))
            .collect::<Vec<_>>();
        futures_util::stream::iter(futures)
            .buffered(3)
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .filter_map(|msg| msg.inspect_err(|e| log::debug!("{e}")).ok())
            .collect()
    }

    async fn message_into_gemini_message(
        &self,
        _bot: &Bot,
        msg: &Message,
    ) -> Result<GeminiMessage> {
        let message_id = msg.id;
        let (user_name, user_id) = msg
            .from
            .as_ref()
            .map(|u| (u.full_name(), u.id))
            .ok_or_else(|| anyhow!("Message has no author"))?;
        let MessageKind::Common(msg) = msg.kind.clone() else {
            return Err(anyhow!("Unsupported message type (Not Common)"));
        };
        let parts = match msg.media_kind.clone() {
            MediaKind::Text(text) => {
                let info = MessageInfo {
                    user_name,
                    user_id,
                    message_content: &text.text,
                    message_id,
                };
                vec![GeminiPart::from(serde_json::to_string(&info)?)]
            }
            _ => return Err(anyhow!("Unsupported media type (Not Text)")),
        };
        let role = if user_id.eq(&self.me.id) {
            GeminiRole::Model
        } else {
            GeminiRole::User
        };
        //         MediaKind::Photo(photo) => {
        //             let file_id = photo
        //                 .photo
        //                 .iter()
        //                 .max_by_key(|p| p.file.size)
        //                 .unwrap()
        //                 .file
        //                 .id
        //                 .clone();
        //             let file_url = bot
        //                 .get_file(file_id.as_str())
        //                 .await
        //                 .inspect_err(|e| log::debug!("{e}"))
        //                 .map(|url| (url.path));
        //             todo!()
        //         }

        Ok(GeminiMessage::new(role, parts))
    }

    fn sanitize_text(s: &str) -> String {
        ["<p>", "</p>", "<br />", "<li>", "</li>", "<ol>", "</ol>"]
            .iter()
            .fold(markdown::to_html(s), |s, pattern| s.replace(pattern, ""))
    }
}

async fn handle_message(
    bot: Bot,
    msg: Message,
    state: Arc<BotState>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    log::debug!("{msg:?}");
    log::debug!("ChatId: {}", msg.chat.id);
    if state.should_reply(&msg) {
        let reply = state.get_gemini_reply(&bot, &msg).await;
        log::debug!("Reply: {reply}");
        if let Err(error) = bot
            .send_message(msg.chat.id, reply)
            .parse_mode(ParseMode::Html)
            .await
        {
            log::error!("Failed to send message: {error}");
        }
    }
    let mut msg_cache = state.msg_cache.lock().await;
    msg_cache.add(msg);
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let cli = parse_cli();
    let config = config::load_config(&cli.config)?;

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
