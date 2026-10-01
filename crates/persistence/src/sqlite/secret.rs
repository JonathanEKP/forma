//! The token machinery personal access tokens proved, shared with the service
//! credential (Pontis plan §10(b) D1) rather than copied into it.
//!
//! Each class keeps its own prefix, and that is the point: `require_bearer`
//! dispatches on the prefix alone, so two classes that shared one would be one
//! class with two meanings. What they share is only how a secret is minted,
//! indexed and compared.

use blake3::Hash;
use uuid::Uuid;

use crate::error::{PersistenceError, Result};

/// Hex characters after the class prefix.
pub(crate) const SECRET_HEX_LEN: usize = 64;

/// How much of the secret the stored, displayable prefix keeps.
const DISPLAY_SECRET_CHARS: usize = 8;

/// A fresh token for `prefix`, with the part of it that may be stored and shown.
pub(crate) fn mint(prefix: &str) -> Result<(String, String)> {
    // UUID v4 contributes 122 random bits after its fixed version/variant
    // bits. Two draw 244 bits through getrandom without another dependency.
    let token = format!(
        "{prefix}{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    );
    let display = token
        .get(..prefix.len() + DISPLAY_SECRET_CHARS)
        .ok_or_else(|| PersistenceError::Storage("generated token was unexpectedly short".into()))?
        .to_string();
    Ok((token, display))
}

/// The stored prefix a well-formed `presented` token would be indexed under.
///
/// `None` for anything that is not exactly this class's shape, so a malformed
/// value never reaches the database.
pub(crate) fn candidate_prefix<'a>(presented: &'a str, prefix: &str) -> Option<&'a str> {
    let secret = presented.strip_prefix(prefix)?;
    if secret.len() != SECRET_HEX_LEN || !secret.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }

    presented.get(..prefix.len() + DISPLAY_SECRET_CHARS)
}

pub(crate) fn hash_hex(token: &str) -> String {
    blake3::hash(token.as_bytes()).to_hex().to_string()
}

/// Whether a stored hash is the hash of what was presented.
///
/// A prefix is only an index hint. The full BLAKE3 `Hash` comparison is
/// constant-time, so a wrong secret cannot be found byte by byte.
pub(crate) fn stored_matches(stored_hex: &str, presented: &Hash) -> Result<bool> {
    let stored = Hash::from_hex(stored_hex).map_err(|error| {
        PersistenceError::Storage(format!("stored credential hash is invalid: {error}"))
    })?;
    Ok(stored == *presented)
}
