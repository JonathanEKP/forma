//! Configuration (plan section 59). Every value is overridable by environment,
//! and the defaults keep the server local-only (plan section 70).

use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Config {
    pub bind: String,
    pub data_dir: PathBuf,
    pub database_url: String,
    pub font_dir: PathBuf,
    pub log: String,
    /// Directory of the built React app, served when it exists.
    pub web_dir: PathBuf,
}

fn env_or(key: &str, fallback: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| fallback.to_string())
}

impl Config {
    pub fn from_env() -> Self {
        let data_dir = PathBuf::from(env_or("FORMA_DATA_DIR", "./data"));
        Self {
            // 127.0.0.1 by default, never 0.0.0.0: remote mode is opt-in.
            bind: env_or("FORMA_BIND", "127.0.0.1:4765"),
            database_url: env_or("FORMA_DATABASE_URL", "sqlite://data/forma.db"),
            font_dir: PathBuf::from(env_or("FORMA_FONT_DIR", "./data/fonts")),
            log: env_or("FORMA_LOG", "info"),
            web_dir: PathBuf::from(env_or("FORMA_WEB_DIR", "./apps/web/dist")),
            data_dir,
        }
    }
}
