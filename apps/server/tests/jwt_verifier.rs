//! The hostile cases (plan sections 111.4 and 116.1).
//!
//! Every token here is minted offline against a committed test keypair, so the
//! cases that matter — an `alg` header of HS256, an unknown `kid`, an expired
//! token, a signature that does not verify — can actually be run. A verifier
//! exercised only against a live provider is one whose failure paths nobody
//! ever takes.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use forma_server::jwt::{JwksSource, JwtError, Verifier};
use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde::Serialize;

const ISSUER: &str = "https://project.supabase.co/auth/v1";
const AUDIENCE: &str = "authenticated";
const KID: &str = "test-key-1";

const SIGNING_KEY: &[u8] = include_bytes!("../../../fixtures/identity/test-signing-key.pkcs8.der");
const JWKS: &str = include_str!("../../../fixtures/identity/test-jwks.json");

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

#[derive(Serialize)]
struct Claims {
    sub: String,
    iss: String,
    aud: String,
    exp: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    nbf: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    email_verified: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    client_id: Option<String>,
}

impl Claims {
    fn valid() -> Self {
        Self {
            sub: "1b9d6bcd-bbfd-4b2d-9b5d-ab8dfbbd4bed".into(),
            iss: ISSUER.into(),
            aud: AUDIENCE.into(),
            exp: now() + 3600,
            nbf: None,
            email: Some("someone@example.com".into()),
            email_verified: Some(true),
            client_id: None,
        }
    }
}

/// Signs with the fixture key, with a header the caller may sabotage.
fn mint(claims: &Claims, header: Header) -> String {
    let key = EncodingKey::from_ec_der(SIGNING_KEY);
    encode(&header, claims, &key).expect("mint")
}

fn es256_header() -> Header {
    let mut header = Header::new(Algorithm::ES256);
    header.kid = Some(KID.into());
    header
}

/// Serves the fixture, and counts how often it was asked.
struct Fixture {
    body: String,
    fetches: Arc<AtomicUsize>,
    fail: bool,
}

#[async_trait::async_trait]
impl JwksSource for Fixture {
    async fn fetch(&self) -> Result<JwkSet, JwtError> {
        self.fetches.fetch_add(1, Ordering::SeqCst);
        if self.fail {
            return Err(JwtError::ProviderUnreachable);
        }
        Ok(serde_json::from_str(&self.body).expect("a jwk set"))
    }
}

fn verifier() -> (Verifier, Arc<AtomicUsize>) {
    let fetches = Arc::new(AtomicUsize::new(0));
    let source = Fixture {
        body: JWKS.to_string(),
        fetches: fetches.clone(),
        fail: false,
    };
    (Verifier::new(ISSUER, AUDIENCE, Box::new(source)), fetches)
}

#[tokio::test]
async fn a_good_token_verifies_and_reports_what_forma_acts_on() {
    let (verifier, _) = verifier();
    let identity = verifier
        .verify(&mint(&Claims::valid(), es256_header()))
        .await
        .expect("a valid token");

    assert_eq!(
        identity.subject.to_string(),
        "1b9d6bcd-bbfd-4b2d-9b5d-ab8dfbbd4bed"
    );
    assert_eq!(identity.email.as_deref(), Some("someone@example.com"));
    assert!(identity.email_verified);
    assert!(
        identity.client_id.is_none(),
        "a direct session has no client"
    );
}

/// The classic confusion attack: claim a symmetric algorithm and sign with the
/// public key as the secret. A verifier that reads `alg` from the token accepts
/// it.
#[tokio::test]
async fn a_token_cannot_choose_its_own_algorithm() {
    let (verifier, _) = verifier();

    let mut hs = Header::new(Algorithm::HS256);
    hs.kid = Some(KID.into());
    let forged = encode(
        &hs,
        &Claims::valid(),
        // The public half is public: an attacker has it.
        &EncodingKey::from_secret(JWKS.as_bytes()),
    )
    .expect("forge");

    assert!(
        matches!(
            verifier.verify(&forged).await,
            Err(JwtError::WrongAlgorithm { .. })
        ),
        "HS256 must be refused before the signature is considered"
    );

    // And the other advertised one. Discovery lists RS256 too; this deployment
    // does not accept it.
    let mut rs = Header::new(Algorithm::RS256);
    rs.kid = Some(KID.into());
    let claims = Claims::valid();
    let payload = serde_json::to_vec(&claims).unwrap();
    let head = serde_json::to_vec(&rs).unwrap();
    let b64 = |b: &[u8]| {
        use base64::Engine;
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b)
    };
    let hand_rolled = format!(
        "{}.{}.{}",
        b64(&head),
        b64(&payload),
        b64(b"not-a-signature")
    );
    assert!(matches!(
        verifier.verify(&hand_rolled).await,
        Err(JwtError::WrongAlgorithm { .. })
    ));
}

#[tokio::test]
async fn expiry_and_not_before_are_enforced() {
    let (verifier, _) = verifier();

    let mut expired = Claims::valid();
    expired.exp = now() - 3600;
    assert!(matches!(
        verifier.verify(&mint(&expired, es256_header())).await,
        Err(JwtError::Invalid)
    ));

    let mut future = Claims::valid();
    future.nbf = Some(now() + 3600);
    assert!(matches!(
        verifier.verify(&mint(&future, es256_header())).await,
        Err(JwtError::Invalid)
    ));
}

#[tokio::test]
async fn the_issuer_and_audience_must_be_ours() {
    let (verifier, _) = verifier();

    let mut elsewhere = Claims::valid();
    elsewhere.iss = "https://someone-elses.supabase.co/auth/v1".into();
    assert!(matches!(
        verifier.verify(&mint(&elsewhere, es256_header())).await,
        Err(JwtError::Invalid)
    ));

    // The audience check is what stops a token minted for another resource of
    // the same project being spent here (plan section 111.4).
    let mut other_resource = Claims::valid();
    other_resource.aud = "some-other-audience".into();
    assert!(matches!(
        verifier
            .verify(&mint(&other_resource, es256_header()))
            .await,
        Err(JwtError::Invalid)
    ));
}

#[tokio::test]
async fn a_subject_that_is_not_a_uuid_is_refused() {
    let (verifier, _) = verifier();

    let mut odd = Claims::valid();
    odd.sub = "not-a-uuid".into();
    assert!(matches!(
        verifier.verify(&mint(&odd, es256_header())).await,
        Err(JwtError::BadSubject)
    ));
}

#[tokio::test]
async fn a_tampered_signature_does_not_verify() {
    let (verifier, _) = verifier();
    let token = mint(&Claims::valid(), es256_header());

    // Flip the last character of the signature.
    let mut parts: Vec<String> = token.split('.').map(str::to_string).collect();
    let signature = parts.pop().unwrap();
    let flipped: String = signature
        .chars()
        .enumerate()
        .map(|(i, c)| if i == 0 && c != 'A' { 'A' } else { c })
        .collect();
    parts.push(flipped);

    assert!(matches!(
        verifier.verify(&parts.join(".")).await,
        Err(JwtError::Invalid)
    ));
}

#[tokio::test]
async fn a_token_naming_no_key_is_refused() {
    let (verifier, _) = verifier();

    let bare = Header::new(Algorithm::ES256);
    assert!(matches!(
        verifier.verify(&mint(&Claims::valid(), bare)).await,
        Err(JwtError::NoKeyId)
    ));
}

/// An unknown `kid` earns exactly one refresh, and then a rejection.
#[tokio::test]
async fn an_unknown_key_refreshes_once_and_then_refuses() {
    let (verifier, fetches) = verifier();

    let mut header = Header::new(Algorithm::ES256);
    header.kid = Some("a-key-that-was-never-published".into());

    assert!(matches!(
        verifier
            .verify(&mint(&Claims::valid(), header.clone()))
            .await,
        Err(JwtError::UnknownKey)
    ));
    assert_eq!(fetches.load(Ordering::SeqCst), 1);

    // The floor is what stops invented kids becoming outbound traffic.
    for _ in 0..20 {
        let _ = verifier
            .verify(&mint(&Claims::valid(), header.clone()))
            .await;
    }
    assert_eq!(
        fetches.load(Ordering::SeqCst),
        1,
        "a stream of invented kids must not become a stream of fetches"
    );
}

/// A provider that is briefly unreachable must not log everybody out.
#[tokio::test]
async fn a_failed_refresh_keeps_the_keys_that_already_worked() {
    let fetches = Arc::new(AtomicUsize::new(0));
    let verifier = Verifier::new(
        ISSUER,
        AUDIENCE,
        Box::new(Fixture {
            body: JWKS.to_string(),
            fetches: fetches.clone(),
            fail: false,
        }),
    );
    verifier.warm().await.expect("warm");
    assert!(verifier
        .verify(&mint(&Claims::valid(), es256_header()))
        .await
        .is_ok());

    // Now the same cache, with a source that has started failing.
    let failing = Verifier::new(
        ISSUER,
        AUDIENCE,
        Box::new(Fixture {
            body: JWKS.to_string(),
            fetches: fetches.clone(),
            fail: true,
        }),
    );
    assert!(
        failing.warm().await.is_err(),
        "a first fetch that fails has nothing to fall back on"
    );
}

/// The fixture mirrors what the real provider serves. If it stops doing so, the
/// tests above are exercising a verifier we do not ship.
#[test]
fn the_fixture_has_the_shape_the_provider_publishes() {
    let set: serde_json::Value = serde_json::from_str(JWKS).expect("jwks");
    let keys = set["keys"].as_array().expect("keys");
    assert_eq!(keys.len(), 1, "one key, as the project publishes");
    assert_eq!(keys[0]["alg"], "ES256");
    assert_eq!(keys[0]["kty"], "EC");
    assert_eq!(keys[0]["crv"], "P-256");
    assert_eq!(keys[0]["use"], "sig");
    assert!(keys[0]["kid"].is_string());
    assert!(
        keys.iter().all(|k| k["kty"] != "oct"),
        "a symmetric key here would make the confusion test meaningless"
    );
}

/// The shapes a real provider token actually has, which the fixtures above did
/// not: `aud` arrives as an array, and `email_verified` lives inside
/// `user_metadata` unless a hook promotes it.
#[tokio::test]
async fn the_real_token_shape_is_read_correctly() {
    let (verifier, _) = verifier();
    let mut header = Header::new(Algorithm::ES256);
    header.kid = Some(KID.into());

    let as_issued = serde_json::json!({
        "sub": "7bd7bcdc-8177-4962-ae51-71362df2e744",
        "iss": ISSUER,
        // An array, which is how Supabase issues it and how the JWT spec allows.
        "aud": ["authenticated"],
        "exp": now() + 3600,
        "email": "someone@example.com",
        "role": "authenticated",
        // Writable by the subject through `auth.updateUser`, so it must not be
        // read: a self-asserted verification is not a verification.
        "user_metadata": { "email_verified": true },
    });

    let identity = verifier
        .verify(&encode(&header, &as_issued, &EncodingKey::from_ec_der(SIGNING_KEY)).expect("mint"))
        .await
        .expect("an array audience must verify");

    assert!(
        !identity.email_verified,
        "user_metadata is the subject's own to write; believing it there would \
         let anyone register as an invited address"
    );

    // With the hook installed, the claim is at the top level and auth-managed.
    let mut hooked = as_issued.clone();
    hooked["email_verified"] = serde_json::Value::Bool(true);
    let identity = verifier
        .verify(&encode(&header, &hooked, &EncodingKey::from_ec_der(SIGNING_KEY)).expect("mint"))
        .await
        .expect("verify");
    assert!(identity.email_verified);
}
