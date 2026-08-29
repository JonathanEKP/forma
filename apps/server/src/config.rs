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
    /// Plan section 57. Set it and every API call needs a bearer token; leave it
    /// unset and the server is local-only, which `binding_is_safe` enforces.
    pub remote_token: Option<String>,
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
            remote_token: std::env::var("FORMA_REMOTE_TOKEN")
                .ok()
                .map(|token| token.trim().to_string())
                .filter(|token| !token.is_empty()),
            data_dir,
        }
    }

    /// Whether this bind address may be served with the configured credentials.
    ///
    /// Reaching the network without a token is the one configuration that must
    /// not start. Plan section 70 says loopback by default and remote by
    /// explicit opt-in; setting `FORMA_REMOTE_TOKEN` *is* that opt-in, so the
    /// two are checked together rather than left as separate switches somebody
    /// can half-set.
    ///
    /// An address that does not parse is treated as remote. A hostname might
    /// resolve anywhere, and guessing in the permissive direction is the wrong
    /// way round for this question.
    pub fn binding_is_safe(&self) -> Result<(), String> {
        if self.remote_token.is_some() {
            return Ok(());
        }

        let loopback = self
            .bind
            .parse::<std::net::SocketAddr>()
            .map(|address| address.ip().is_loopback())
            .unwrap_or(false);

        if loopback {
            Ok(())
        } else {
            Err(format!(
                "FORMA_BIND is {} , which is reachable from the network, and FORMA_REMOTE_TOKEN \
                 is not set. Set a token (plan section 57) or bind 127.0.0.1.",
                self.bind
            ))
        }
    }
}
