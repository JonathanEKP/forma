//! Verifying the identity provider's tokens (plan section 111.4).
//!
//! Two rules govern everything here, and both are absolute rather than
//! defaults:
//!
//! **The token never says how to check itself.** Its `alg` header is read only
//! to be compared against what this deployment accepts, and a mismatch is a
//! rejection before the signature is considered. The configured project's OIDC
//! discovery advertises `RS256`, `HS256` and `ES256` while its JWKS publishes a
//! single ES256 key — that list is the provider's generic capability, not this
//! deployment's policy, and a verifier that reads either one is how the
//! algorithm-confusion attack works.
//!
//! **A failure is a rejection, never a downgrade.** Nothing here falls back to
//! a weaker check when a stricter one cannot be performed.

use std::collections::{HashMap, HashSet};
use std::sync::RwLock;
use std::time::{Duration, Instant};

use design_core::AuthUserId;
use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use serde::Deserialize;

/// The one algorithm this deployment accepts.
const ALGORITHM: Algorithm = Algorithm::ES256;

/// Clock skew allowed on `exp` and `nbf`.
///
/// Small on purpose. These tokens are short-lived and the gap between our clock
/// and the provider's should be seconds; `jsonwebtoken` defaults to sixty, which
/// would keep an expired session usable for a minute after it ended.
const LEEWAY_SECONDS: u64 = 5;

/// The shortest gap between two JWKS fetches.
///
/// An unknown `kid` may trigger a refresh — that is how a key rotation is
/// survived without a restart — and without a floor a stream of tokens carrying
/// invented `kid`s turns every request into an outbound fetch. That is a denial
/// of service against the provider, paid for by someone who did nothing.
const REFRESH_FLOOR: Duration = Duration::from_secs(60);

#[derive(Debug, thiserror::Error)]
pub enum JwtError {
    #[error("the token is malformed")]
    Malformed,
    #[error("this deployment verifies {expected:?} and the token declares {found:?}")]
    WrongAlgorithm {
        expected: Algorithm,
        found: Algorithm,
    },
    #[error("the token names no signing key")]
    NoKeyId,
    #[error("no published key matches the token's kid")]
    UnknownKey,
    #[error("the token did not verify")]
    Invalid,
    #[error("the subject is missing or is not a uuid")]
    BadSubject,
    #[error("the identity provider could not be reached")]
    ProviderUnreachable,
}

/// What a verified token says, reduced to what Forma acts on.
///
/// Deliberately not the raw claims: everything downstream should depend on the
/// few fields with meaning here, so a change in the provider's payload is a
/// change in one place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedIdentity {
    pub subject: AuthUserId,
    pub email: Option<String>,
    /// Whether the provider says it proved the address.
    ///
    /// The invite gate of plan 107.3 matches on the address, so it is only a
    /// gate if the address was proven. Never trust `email` without this.
    ///
    /// **Read from the top level only, and never from `user_metadata`.** A real
    /// Supabase token carries `email_verified` inside `user_metadata`, which is
    /// writable by the subject through `auth.updateUser` — a claim they can set
    /// about themselves. Reading it there would let anyone register as an
    /// invited address and walk in, which is precisely what this gate exists to
    /// stop. `deploy/supabase/access-token-hook.sql` promotes the auth-managed
    /// `email_confirmed_at` to a top-level claim so there is something honest
    /// here to read; without that hook installed this is `false` and the gate
    /// refuses everyone, which is the right way for it to be missing.
    pub email_verified: bool,
    /// Present when an OAuth client obtained the token; null for a direct user
    /// session. That difference is what separates a browser credential from an
    /// agent credential (plan section 112).
    pub client_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Claims {
    sub: String,
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    email_verified: Option<bool>,
    #[serde(default)]
    client_id: Option<String>,
}

/// Where the published keys come from.
///
/// A trait so the tests can serve a fixture. A verifier that can only be
/// exercised against a live provider is one nobody runs the hostile cases
/// against, and the hostile cases are the point.
#[async_trait::async_trait]
pub trait JwksSource: Send + Sync {
    async fn fetch(&self) -> Result<JwkSet, JwtError>;
}

pub struct HttpJwks {
    url: String,
    client: reqwest::Client,
}

impl HttpJwks {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait::async_trait]
impl JwksSource for HttpJwks {
    async fn fetch(&self) -> Result<JwkSet, JwtError> {
        self.client
            .get(&self.url)
            .timeout(Duration::from_secs(10))
            .send()
            .await
            .map_err(|_| JwtError::ProviderUnreachable)?
            .json::<JwkSet>()
            .await
            .map_err(|_| JwtError::ProviderUnreachable)
    }
}

#[derive(Default)]
struct Cache {
    keys: HashMap<String, DecodingKey>,
    last_fetch: Option<Instant>,
}

pub struct Verifier {
    issuer: String,
    audience: String,
    source: Box<dyn JwksSource>,
    cache: RwLock<Cache>,
}

impl Verifier {
    pub fn new(
        issuer: impl Into<String>,
        audience: impl Into<String>,
        source: Box<dyn JwksSource>,
    ) -> Self {
        Self {
            issuer: issuer.into(),
            audience: audience.into(),
            source,
            cache: RwLock::new(Cache::default()),
        }
    }

    /// Verifies a token, or says why it will not.
    pub async fn verify(&self, token: &str) -> Result<VerifiedIdentity, JwtError> {
        // Read the header only to refuse it. The signature is not considered
        // until the algorithm this deployment accepts is the one declared.
        //
        // Measured, so nobody removes this believing it redundant or keeps it
        // believing it is the only thing standing there: with this check and the
        // `Validation` pin below *both* removed, `jsonwebtoken` still refuses an
        // HS256 forgery, because the `DecodingKey` it is handed is an EC key and
        // it will not use one as an HMAC secret. This is defence in depth over
        // that, and it earns its place three ways — it rejects before any
        // signature work happens, it says `WrongAlgorithm` where the library
        // says only `Invalid`, and it does not depend on a third party
        // continuing to enforce key-family matching for us.
        let header = decode_header(token).map_err(|_| JwtError::Malformed)?;
        if header.alg != ALGORITHM {
            return Err(JwtError::WrongAlgorithm {
                expected: ALGORITHM,
                found: header.alg,
            });
        }
        let kid = header.kid.ok_or(JwtError::NoKeyId)?;

        let key = match self.cached(&kid) {
            Some(key) => key,
            // An unknown kid is the normal shape of a key rotation, so it earns
            // one refresh — subject to the floor above.
            None => {
                self.refresh().await?;
                self.cached(&kid).ok_or(JwtError::UnknownKey)?
            }
        };

        let mut validation = Validation::new(ALGORITHM);
        // Set again explicitly. `Validation::new` already fills it, and stating
        // it here means a future edit to the constructor cannot widen what is
        // accepted without touching this line.
        validation.algorithms = vec![ALGORITHM];
        validation.set_issuer(&[&self.issuer]);
        validation.set_audience(&[&self.audience]);
        validation.validate_exp = true;
        validation.validate_nbf = true;
        validation.leeway = LEEWAY_SECONDS;
        validation.required_spec_claims =
            HashSet::from(["exp".into(), "iss".into(), "aud".into(), "sub".into()]);

        let data = decode::<Claims>(token, &key, &validation).map_err(|_| JwtError::Invalid)?;

        let subject = uuid::Uuid::parse_str(&data.claims.sub)
            .map(AuthUserId)
            .map_err(|_| JwtError::BadSubject)?;

        Ok(VerifiedIdentity {
            subject,
            email: data.claims.email,
            email_verified: data.claims.email_verified.unwrap_or(false),
            client_id: data.claims.client_id,
        })
    }

    fn cached(&self, kid: &str) -> Option<DecodingKey> {
        self.cache.read().ok()?.keys.get(kid).cloned()
    }

    /// Replaces the cached keys, unless it was tried too recently.
    ///
    /// A fetch that fails leaves the cache alone. Keys that verified a moment
    /// ago still verify: a provider being briefly unreachable must not log
    /// everybody out.
    async fn refresh(&self) -> Result<(), JwtError> {
        {
            let cache = self.cache.read().map_err(|_| JwtError::UnknownKey)?;
            if cache
                .last_fetch
                .is_some_and(|at| at.elapsed() < REFRESH_FLOOR)
            {
                return Ok(());
            }
        }

        let fetched = self.source.fetch().await;

        let mut cache = self.cache.write().map_err(|_| JwtError::UnknownKey)?;
        // Stamped whether or not the fetch succeeded, so a provider that is down
        // is asked at the same rate as one that is up rather than on every call.
        cache.last_fetch = Some(Instant::now());

        let set = fetched?;
        let mut keys = HashMap::new();
        for jwk in &set.keys {
            let Some(kid) = jwk.common.key_id.clone() else {
                continue;
            };
            if let Ok(key) = DecodingKey::from_jwk(jwk) {
                keys.insert(kid, key);
            }
        }

        // An empty answer is not a reason to forget what we know. A provider
        // serving `{"keys":[]}` mid-rotation would otherwise revoke every
        // session at once.
        if !keys.is_empty() {
            cache.keys = keys;
        }
        Ok(())
    }

    /// Loads the keys once, at startup, so the first request does not pay for
    /// it and a misconfigured JWKS URL is noticed before anyone signs in.
    pub async fn warm(&self) -> Result<(), JwtError> {
        self.refresh().await
    }
}
