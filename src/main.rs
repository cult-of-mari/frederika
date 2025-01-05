use aho_corasick::AhoCorasick;
use anyhow::Result;
use futures_util::StreamExt;
use google_gemini::{
    GeminiClient, GeminiMessage, GeminiRequest, GeminiRole, GeminiSafetySetting,
    GeminiSafetyThreshold, GeminiSystemPart, Part, TextPart,
};
use serde::Serialize;
use std::{borrow::Borrow, sync::Arc};
use teloxide::{
    dispatching::UpdateFilterExt,
    prelude::*,
    types::{ChatKind, Me, MediaKind, MessageId, MessageKind, ParseMode},
};
use tokio::sync::Mutex;

mod attachment;
mod cli;
mod config;
mod msg_cache;

use attachment::GeminiAttachment;
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
    http_client: reqwest::Client,
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
            http_client: reqwest::Client::new(),
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
                || self.name_matcher.is_match(msg.text().unwrap_or_default())
                || self
                    .name_matcher
                    .is_match(msg.caption().unwrap_or_default()))
    }

    async fn get_gemini_reply(&self, bot: &Bot, msg: &Message) -> String {
        let mut request = GeminiRequest::default();

        request
            .system_instruction
            .get_or_insert_default()
            .parts
            .push(GeminiSystemPart {
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
        let response = self.gemini.generate(request).await;
        let text = match response.as_deref() {
            Ok(
                [Part::Text(TextPart {
                    text,
                    thought: false,
                })],
            ) => text.to_string(),
            Ok(_parts) => "```\nissue\n```\nReport this issue to the admins".to_string(),
            Err(error) => {
                format!("```\n{error}\n```\nReport this issue to the admins")
            }
        };
        BotState::sanitize_text(&text)
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

    async fn message_into_gemini_message(&self, bot: &Bot, msg: &Message) -> Result<GeminiMessage> {
        let message_id = msg.id;
        let message_content = msg.text().unwrap_or(msg.caption().unwrap_or_default());
        let (user_name, user_id) = msg
            .from
            .as_ref()
            .map(|u| (u.full_name(), u.id))
            .ok_or_else(|| anyhow::anyhow!("Message has no author"))?;
        let MessageKind::Common(msg) = msg.kind.borrow() else {
            return Err(anyhow::anyhow!("Unsupported message type (Not Common)"));
        };
        let info = MessageInfo {
            user_name,
            user_id,
            message_content,
            message_id,
        };
        let mut parts = vec![Part::from(serde_json::to_string(&info)?)];
        let attachment = match msg.media_kind.clone() {
            MediaKind::Photo(photo) => {
                let file_meta = photo
                    .photo
                    .iter()
                    .max_by_key(|p| p.file.size)
                    .unwrap()
                    .file
                    .borrow();
                let file = bot.get_file(file_meta.id.as_str()).await?;
                let url = format!(
                    "https://api.telegram.org/file/{}/{}",
                    bot.token(),
                    file.path
                );
                self.url_to_gemini_attachment(url, file.path)
                    .await
                    .inspect_err(|e| {
                        log::debug!("Couldn't submit an attachment: {e}");
                    })
                    .ok()
            }
            media_kind => {
                log::debug!("Unsupported media kind {media_kind:?}");
                None
            }
        };
        if let Some(attachment) = attachment {
            parts.push(Part::from(attachment));
        }
        let role = if user_id.eq(&self.me.id) {
            GeminiRole::Model
        } else {
            GeminiRole::User
        };
        Ok(GeminiMessage::new(role, parts))
    }

    async fn url_to_gemini_attachment(
        &self,
        url: String,
        file_name: String,
    ) -> Result<GeminiAttachment> {
        let mime = mime_guess::from_path(url.clone()).first().unwrap();
        let mime_str = mime.to_string();
        let bytes = self
            .http_client
            .get(url.clone())
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?;
        let content_length = bytes.len() as u32;
        let url = self
            .gemini
            .create_file(&file_name, content_length, mime_str.as_str())
            .await?;
        let url = self
            .gemini
            .upload_file(url, content_length, bytes.into())
            .await?;
        Ok(GeminiAttachment {
            uri: url,
            content_type: mime_str.into(),
        })
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
