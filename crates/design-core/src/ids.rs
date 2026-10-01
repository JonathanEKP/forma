//! Typed ID wrappers (plan section 11.1): IDs must not be interchangeable strings.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

macro_rules! typed_id {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
        #[serde(transparent)]
        #[ts(export, type = "string")]
        pub struct $name(pub Uuid);

        impl $name {
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", self.0)
            }
        }
    };
}

typed_id!(ProjectId);
typed_id!(DesignId);
typed_id!(PageId);
typed_id!(LayerId);
typed_id!(AssetId);
typed_id!(BrandId);
typed_id!(AssetRequestId);
typed_id!(ExportId);
typed_id!(TemplateId);

// Plan section 107. `AuthUserId` is the identity provider's subject rather than
// anything Forma mints, but it is still an id and must not be an interchangeable
// string: confusing it with an `AccountId` is how a tenancy check reads the
// wrong column and passes.
typed_id!(AccountId);
typed_id!(AuthUserId);
typed_id!(ApiKeyId);

// The Pontis sidecar's credential (plan D1). Its own id type rather than an
// `ApiKeyId`, because the two are different credential classes, and history
// that names one must never be readable as the other.
typed_id!(ServiceCredentialId);

/// A Pontis invocation id: the idempotency key Boundary F carries (Pontis plan
/// §10(b) D6).
///
/// Not a `typed_id!`, because Forma does not mint it: the Pontis runtime assigns
/// it and keeps it across an approval round trip, so it is an opaque string of
/// someone else's shape. It is still its own type, so it cannot be confused
/// with a design or layer id that happens to be a string at the boundary.
///
/// The bounds are the reference host's (non-empty, at most 128 bytes) plus no
/// control characters, which have no business in a key a person may have to
/// read out of a table.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct InvocationId(String);

impl InvocationId {
    pub const MAX_LEN: usize = 128;

    pub fn parse(raw: &str) -> Result<Self, InvalidInvocationId> {
        if raw.is_empty() || raw.len() > Self::MAX_LEN || raw.chars().any(char::is_control) {
            return Err(InvalidInvocationId);
        }
        Ok(Self(raw.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for InvocationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Deserializing goes through `parse`, so a malformed key cannot enter by a
/// side door.
impl<'de> Deserialize<'de> for InvocationId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Self::parse(&raw).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidInvocationId;

impl std::fmt::Display for InvalidInvocationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "an invocation id must be 1 to {} bytes with no control characters",
            InvocationId::MAX_LEN
        )
    }
}

impl std::error::Error for InvalidInvocationId {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_invocation_id_has_the_reference_hosts_bounds() {
        assert!(InvocationId::parse("inv_01").is_ok());
        assert!(InvocationId::parse(&"a".repeat(128)).is_ok());

        assert!(InvocationId::parse("").is_err());
        assert!(InvocationId::parse(&"a".repeat(129)).is_err());
        assert!(InvocationId::parse("inv\n01").is_err());

        let decoded: Result<InvocationId, _> = serde_json::from_str("\"\"");
        assert!(decoded.is_err(), "deserializing must not bypass parse");
    }
}
