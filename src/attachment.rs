use google_gemini::content::{FileDataPart, Part, TaggedPart};
use std::borrow::Cow;

/// Registered and uploaded file reference
#[derive(Debug, Clone)]
pub struct GeminiAttachment {
    pub uri: String,
    pub content_type: Cow<'static, str>,
}

impl From<GeminiAttachment> for Part {
    fn from(value: GeminiAttachment) -> Self {
        Self::TaggedPart(TaggedPart::FileData(FileDataPart {
            mime_type: value.content_type.to_string(),
            file_uri: value.uri,
        }))
    }
}
