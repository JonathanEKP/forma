//! Configuration (plan section 59). Every value is overridable by environment,
//! and the defaults keep the server local-only (plan section 70).

use std::path::PathBuf;

/// How a caller proves who they are (plan section 106.2).
///
/// This is a deployment decision, not a request-level one, and it exists because
/// "no credential is configured" must stop being a synonym for "no credential is
/// required". Local Forma has no identity and needs none; a hosted one has
/// identity and must refuse to run without it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthMode {
    /// Trusted-local: the security boundary is the process that started Forma.
    Local,
    /// Hosted accounts, browser sessions and OAuth over MCP.
    Supabase,
}

impl AuthMode {
    fn parse(raw: &str) -> Result<Self, String> {
        match raw.trim().to_lowercase().as_str() {
            "local" => Ok(AuthMode::Local),
            "supabase" => Ok(AuthMode::Supabase),
            other => Err(format!(
                "FORMA_AUTH_MODE is {other:?}; it must be \"local\" or \"supabase\""
            )),
        }
    }
}

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
    /// Plan section 106.2. Defaults to `local`, so an existing deployment keeps
    /// the behaviour it has until it opts in.
    pub auth_mode: Result<AuthMode, String>,
    /// Where hosted identity is verified. Required in `supabase` mode.
    pub supabase_url: Option<String>,
    pub supabase_issuer: Option<String>,
    pub supabase_jwks_url: Option<String>,
    /// The `aud` a browser session token must carry.
    ///
    /// **Not measured.** Obtaining one needs a real sign-in, and this project's
    /// mailer does not auto-confirm, so the first person to log in is the first
    /// chance to read it. `authenticated` is Supabase's documented value for a
    /// direct user session and is the default here.
    ///
    /// It is configuration rather than a constant precisely because it is a
    /// guess: if it is wrong the verifier **fails closed** — valid tokens are
    /// refused, nobody gets in, and it is corrected with an environment
    /// variable rather than a release. A wrong guess in the other direction
    /// would have been a hole; this one is a lockout, which is the right way
    /// for an unverified assumption to break.
    pub supabase_audience: String,
    /// The MCP resource an agent's token must be addressed to (plan §114.1),
    /// e.g. `https://forma.hintly.online/mcp`. The access-token hook appends the
    /// OAuth client id, so this is the prefix.
    pub mcp_resource_url: Option<String>,
}

/// An environment value that is absent when unset or blank.
fn optional(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
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
            // Carried as a Result rather than defaulted: a mode nobody can
            // parse must not quietly become the permissive one, and
            // `binding_is_safe` is the single place that refuses to start.
            auth_mode: optional("FORMA_AUTH_MODE")
                .map(|raw| AuthMode::parse(&raw))
                .unwrap_or(Ok(AuthMode::Local)),
            supabase_url: optional("FORMA_SUPABASE_URL"),
            supabase_issuer: optional("FORMA_SUPABASE_ISSUER"),
            supabase_jwks_url: optional("FORMA_SUPABASE_JWKS_URL"),
            supabase_audience: env_or("FORMA_SUPABASE_AUDIENCE", "authenticated"),
            mcp_resource_url: optional("FORMA_MCP_RESOURCE_URL"),
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
    /// The mode this process runs in.
    ///
    /// Only ever called after `binding_is_safe` has accepted the configuration,
    /// which is what makes the fallback unreachable rather than permissive.
    pub fn mode(&self) -> AuthMode {
        self.auth_mode.clone().unwrap_or(AuthMode::Local)
    }

    pub fn binding_is_safe(&self) -> Result<(), String> {
        // Hosted mode does not get to fall back on the bind address at all.
        //
        // The old check rested on "loopback means only this machine can reach
        // it", and that premise stopped being true the day a reverse proxy was
        // put in front of loopback: the whole internet reaches it, through
        // Caddy. It was masked because FORMA_REMOTE_TOKEN happened to be set, so
        // the function returned on its first line and its reasoning was never
        // reached. Plan section 113.1 retires that token once the legacy account
        // is claimed, and without this the deployment would come up serving
        // everything unauthenticated with nothing in any log saying so.
        //
        // So in `supabase` mode the question is not where we bind, but whether
        // identity is actually configured.
        let mode = self.auth_mode.clone()?;

        if mode == AuthMode::Supabase {
            let missing: Vec<&str> = [
                ("FORMA_SUPABASE_ISSUER", self.supabase_issuer.is_none()),
                ("FORMA_SUPABASE_JWKS_URL", self.supabase_jwks_url.is_none()),
            ]
            .iter()
            .filter(|(_, absent)| *absent)
            .map(|(name, _)| *name)
            .collect();

            return if missing.is_empty() {
                Ok(())
            } else {
                Err(format!(
                    "FORMA_AUTH_MODE is supabase but {} is not set. A hosted deployment with no \
                     way to verify identity would serve every request unauthenticated.",
                    missing.join(" and ")
                ))
            };
        }

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
