//! The portable document (plan section 14).
//!
//! It deliberately carries no `project_id`, no `design_id`, no database version,
//! no local paths and no remote URLs — only portable references.

use serde::{Deserialize, Serialize};

use crate::error::DomainError;
use crate::page::Page;

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormatPreset {
    InstagramPortrait,
    Square,
    Story,
    Landscape,
    Custom,
}

/// Plan section 14: the format belongs to the whole design, not to each page,
/// so every slide of a carousel shares one canvas.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DesignFormat {
    pub preset: FormatPreset,
    pub width: f32,
    pub height: f32,
}

impl DesignFormat {
    pub fn from_preset(preset: FormatPreset) -> Self {
        let (width, height) = match preset {
            FormatPreset::InstagramPortrait => (1080.0, 1350.0),
            FormatPreset::Square => (1080.0, 1080.0),
            FormatPreset::Story => (1080.0, 1920.0),
            FormatPreset::Landscape => (1920.0, 1080.0),
            FormatPreset::Custom => (1080.0, 1080.0),
        };
        Self {
            preset,
            width,
            height,
        }
    }

    pub fn validate(&self) -> Result<(), DomainError> {
        if self.width <= 0.0 || self.height <= 0.0 {
            return Err(DomainError::InvalidDimension(format!(
                "canvas must be positive, got {}x{}",
                self.width, self.height
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DesignDocument {
    pub schema_version: u32,
    pub format: DesignFormat,
    #[serde(default)]
    pub pages: Vec<Page>,
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub metadata: serde_json::Map<String, serde_json::Value>,
}

impl DesignDocument {
    pub fn new(format: DesignFormat) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            format,
            pages: Vec::new(),
            metadata: serde_json::Map::new(),
        }
    }

    /// Plan section 76. V1 only parses schema 1, but the entry point exists so
    /// the JSON migration path is not bolted on later.
    pub fn migrate(value: serde_json::Value) -> Result<Self, DomainError> {
        let version = value
            .get("schema_version")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;

        match version {
            SCHEMA_VERSION => serde_json::from_value(value)
                .map_err(|e| DomainError::InvalidDimension(e.to_string())),
            other => Err(DomainError::UnsupportedSchemaVersion(other)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_match_the_plan() {
        let f = DesignFormat::from_preset(FormatPreset::InstagramPortrait);
        assert_eq!((f.width, f.height), (1080.0, 1350.0));
        assert_eq!(
            DesignFormat::from_preset(FormatPreset::Story).height,
            1920.0
        );
    }

    #[test]
    fn rejects_a_non_positive_canvas() {
        let mut f = DesignFormat::from_preset(FormatPreset::Square);
        f.width = 0.0;
        assert!(f.validate().is_err());
    }

    #[test]
    fn refuses_an_unknown_schema_version() {
        let doc = serde_json::json!({ "schema_version": 99, "format": null, "pages": [] });
        assert!(matches!(
            DesignDocument::migrate(doc),
            Err(DomainError::UnsupportedSchemaVersion(99))
        ));
    }
}
