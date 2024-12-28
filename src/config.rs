use figment::{
    providers::{Format, Toml},
    Figment, Result,
};
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct Telegram {
    pub token: String,
    pub cache_size: usize,
}

#[derive(Debug, Deserialize)]
pub struct Gemini {
    pub token: String,
    pub personality: String,
}

#[derive(Debug, Deserialize)]
pub struct Config {
    pub telegram: Telegram,
    pub gemini: Gemini,
}

pub fn load_config(config_path: &Path) -> Result<Config> {
    Figment::new().merge(Toml::file(config_path)).extract()
}
