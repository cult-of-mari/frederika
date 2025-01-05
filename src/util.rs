use anyhow::{anyhow, Result};
use google_gemini::GeminiClient;
use std::path::Path;
use teloxide::{
    prelude::Requester,
    types::{FileMeta, MediaKind},
    Bot,
};

use crate::attachment::GeminiAttachment;

pub fn sanitize_text(s: &str) -> String {
    ["<p>", "</p>", "<br />", "<li>", "</li>", "<ol>", "</ol>"]
        .iter()
        .fold(markdown::to_html(s), |s, pattern| s.replace(pattern, ""))
}

pub fn media_kind_to_file_meta(media_kind: &MediaKind) -> Result<FileMeta> {
    match media_kind {
        MediaKind::Photo(photo) => Ok(photo
            .photo
            .iter()
            .max_by_key(|p| p.file.size)
            .unwrap()
            .file
            .clone()),
        MediaKind::Animation(animation) => Ok(animation.animation.file.clone()),
        media_kind => Err(anyhow!("Unsupported media kind {media_kind:?}")),
    }
}

pub async fn media_kind_to_gemini_attachment(
    bot: &Bot,
    client: &reqwest::Client,
    gemini: &GeminiClient,
    media_kind: &MediaKind,
) -> Result<GeminiAttachment> {
    let file_meta = media_kind_to_file_meta(media_kind)?;
    let file = bot.get_file(file_meta.id.as_str()).await?;
    let url = format!(
        "https://api.telegram.org/file/bot{}/{}",
        bot.token(),
        file.path
    );
    url_to_gemini_attachment(client, gemini, url).await
}

pub async fn url_to_gemini_attachment(
    client: &reqwest::Client,
    gemini: &GeminiClient,
    url: String,
) -> Result<GeminiAttachment> {
    let mime = mime_guess::from_path(url.clone()).first().unwrap();
    let mime_str = mime.to_string();
    let file_name = Path::new(&url)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();
    let bytes = client
        .get(url.clone())
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    let content_length = bytes.len() as u32;
    let url = gemini
        .create_file(&file_name, content_length, mime_str.as_str())
        .await?;
    let url = gemini
        .upload_file(url, content_length, bytes.into())
        .await?;
    Ok(GeminiAttachment {
        uri: url,
        content_type: mime_str.into(),
    })
}
