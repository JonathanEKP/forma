//! Version semantics (plan sections 27 and 29).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A design's version counter.
///
/// It only ever moves forward. Undo restores an older snapshot as a *new*
/// version rather than decrementing, so optimistic concurrency never sees the
/// same version describe two different documents (plan section 29).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[serde(transparent)]
#[ts(export, type = "number")]
pub struct Version(pub u64);

impl Version {
    pub const INITIAL: Self = Self(1);

    pub fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Who performed a mutation (plan section 61).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ActorType {
    Human,
    Agent,
    System,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Actor {
    pub actor_type: ActorType,
    /// For example `agent:claude` or `agent:codex`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor_id: Option<String>,
}

impl Actor {
    pub fn human() -> Self {
        Self {
            actor_type: ActorType::Human,
            actor_id: None,
        }
    }

    pub fn agent(id: impl Into<String>) -> Self {
        Self {
            actor_type: ActorType::Agent,
            actor_id: Some(id.into()),
        }
    }

    pub fn system() -> Self {
        Self {
            actor_type: ActorType::System,
            actor_id: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_only_move_forward() {
        let v = Version::INITIAL;
        assert_eq!(v.next(), Version(2));
        assert!(v.next() > v);
    }
}
