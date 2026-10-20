use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub database: DatabaseConfig,
    pub server: ServerConfig,
    #[serde(default)] pub github: Vec<GithubConfig>,
    #[serde(default)] pub web: Vec<WebConfig>,
    #[serde(default)] pub telegram: Vec<TelegramConfig>,
    #[serde(default)] pub rss: Vec<RssConfig>,
    #[serde(default)] pub price: Vec<PriceConfig>,
    #[serde(default)] pub chainweb: Vec<ChainwebConfig>,
    #[serde(default)] pub github_releases: Vec<GithubReleasesConfig>,
    #[serde(default)] pub github_commits: Vec<GithubCommitsConfig>,
    #[serde(default)] pub social: Vec<SocialConfig>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DatabaseConfig { pub url: String }

#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig { pub port: u16, pub cors_origin: String }

#[derive(Debug, Deserialize, Clone)]
pub struct GithubConfig {
    pub name: String, pub org: String,
    pub poll_interval_secs: u64,
    #[serde(default = "default_true")] pub enabled: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct WebConfig {
    pub name: String, pub url: String,
    pub poll_interval_secs: u64,
    #[serde(default = "default_true")] pub enabled: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct TelegramConfig {
    pub name: String, pub channel: String,
    pub poll_interval_secs: u64,
    #[serde(default)] pub enabled: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RssConfig {
    pub name: String, pub url: String,
    #[serde(default = "default_crypto")] pub category: String,
    pub poll_interval_secs: u64,
    #[serde(default = "default_true")] pub enabled: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct PriceConfig {
    pub name: String,
    pub poll_interval_secs: u64,
    #[serde(default = "default_true")] pub enabled: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ChainwebConfig {
    pub name: String,
    pub network: String,
    #[serde(default)] pub endpoints: Vec<String>,
    pub poll_interval_secs: u64,
    #[serde(default = "default_true")] pub enabled: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct GithubReleasesConfig {
    pub name: String, pub org: String, pub repo: String,
    pub poll_interval_secs: u64,
    #[serde(default = "default_true")] pub enabled: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct GithubCommitsConfig {
    pub name: String, pub org: String, pub repo: String,
    pub poll_interval_secs: u64,
    #[serde(default = "default_true")] pub enabled: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct SocialConfig {
    pub name: String,
    pub poll_interval_secs: u64,
    #[serde(default = "default_true")] pub enabled: bool,
}

fn default_true() -> bool { true }
fn default_crypto() -> String { "crypto".to_string() }

pub fn load(path: &str) -> Result<Config> {
    let s = std::fs::read_to_string(path).with_context(|| format!("read {}", path))?;
    let c: Config = toml::from_str(&s).context("parse oracle.toml")?;
    Ok(c)
}
