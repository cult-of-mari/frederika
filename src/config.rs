use figment::{
    providers::{Format, Toml},
    Figment, Result,
};
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct Telegram {
    pub token: String,
}

#[derive(Debug, Deserialize)]
pub struct Config {
    pub telegram: Telegram,
}

pub fn load_config(config_path: &Path) -> Result<Config> {
    Figment::new().merge(Toml::file(config_path)).extract()
}
