//! The portable document (plan section 14).
//!
//! It deliberately carries no `project_id`, no `design_id`, no database version,
//! no local paths and no remote URLs — only portable references.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::DomainError;
use crate::layer::Layer;
use crate::page::Page;

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum FormatPreset {
    InstagramPortrait,
    Square,
    Story,
    Landscape,
    Custom,
}

/// Plan section 14: the format belongs to the whole design, not to each page,
/// so every slide of a carousel shares one canvas.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
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

/// The persistent identity of a design, kept separate from the portable
/// document it carries (plan section 14). The record knows about the workspace
/// and the version; the document knows neither, which is what lets it become a
/// template or move between projects.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DesignRecord {
    pub id: crate::ids::DesignId,
    pub project_id: crate::ids::ProjectId,
    pub name: String,
    pub version: crate::version::Version,
    pub created_at: String,
    pub updated_at: String,
    pub document: DesignDocument,
}

/// A design without its document, for listings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DesignSummary {
    pub id: crate::ids::DesignId,
    pub project_id: crate::ids::ProjectId,
    pub name: String,
    pub version: crate::version::Version,
    pub format: DesignFormat,
    pub page_count: usize,
    pub created_at: String,
    pub updated_at: String,
}

impl DesignRecord {
    pub fn summary(&self) -> DesignSummary {
        DesignSummary {
            id: self.id,
            project_id: self.project_id,
            name: self.name.clone(),
            version: self.version,
            format: self.document.format,
            page_count: self.document.pages.len(),
            created_at: self.created_at.clone(),
            updated_at: self.updated_at.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DesignDocument {
    pub schema_version: u32,
    pub format: DesignFormat,
    #[serde(default)]
    pub pages: Vec<Page>,
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty")]
    #[ts(type = "Record<string, unknown>")]
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

    /// Structural validation, run after every batch of operations before the
    /// result is persisted (plan section 28).
    ///
    /// These are invariants of the document itself, not design quality: the
    /// aesthetic checks of section 42 belong to the validator crate.
    pub fn validate_structure(&self) -> Result<(), DomainError> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(DomainError::UnsupportedSchemaVersion(self.schema_version));
        }
        self.format.validate()?;

        if self.pages.is_empty() {
            return Err(DomainError::InvalidDocument(
                "a design must keep at least one page".to_string(),
            ));
        }

        let mut page_ids = HashSet::new();
        let mut layer_ids = HashSet::new();
        for page in &self.pages {
            if !page_ids.insert(page.id) {
                return Err(DomainError::InvalidDocument(format!(
                    "page {} appears more than once",
                    page.id
                )));
            }
            for layer in &page.layers {
                // Layer ids are unique across the whole document, not per page,
                // because operations address layers by id alone.
                if !layer_ids.insert(layer.id()) {
                    return Err(DomainError::DuplicateLayerId(layer.id()));
                }
                if let Layer::Text(text) = layer {
                    if let Some(bad) = text.text.chars().find(|c| forbidden_in_xml(*c)) {
                        return Err(DomainError::InvalidDocument(format!(
                            "layer {} contains U+{:04X}, which the renderer's SVG cannot carry",
                            layer.id(),
                            bad as u32
                        )));
                    }
                }
            }
        }
        Ok(())
    }

    /// Plan section 76. V1 only parses schema 1, but the entry point exists so
    /// the JSON migration path is not bolted on later.
    pub fn migrate(value: serde_json::Value) -> Result<Self, DomainError> {
        let version = value
            .get("schema_version")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;

        match version {
            // Any parse failure is a malformed document, whatever the field:
            // reporting it as INVALID_DIMENSION would mislead an agent that
            // branches on the code.
            SCHEMA_VERSION => serde_json::from_value(value)
                .map_err(|e| DomainError::InvalidDocument(e.to_string())),
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

/// Characters XML 1.0 cannot represent **at all** — not escaped, not as a
/// numeric reference. The render path is `DesignDocument -> SVG -> PNG`, so a
/// string carrying one produces a document that validates, refuses to render and
/// cannot be exported. Tab, newline and carriage return are legal and stay:
/// line breaks in a `TextLayer` are explicit newlines (plan section 18).
pub fn forbidden_in_xml(character: char) -> bool {
    matches!(character,
        '\u{0}'..='\u{8}' | '\u{B}' | '\u{C}' | '\u{E}'..='\u{1F}' | '\u{FFFE}' | '\u{FFFF}')
}

#[cfg(test)]
mod xml_safety {
    use super::*;
    use crate::page::Page;
    use crate::{
        DesignColor, DesignFormat, FormatPreset, LayerCommon, LayerId, TextAlign, TextLayer,
        Transform,
    };

    fn document_saying(text: &str) -> DesignDocument {
        let mut document = DesignDocument::new(DesignFormat::from_preset(FormatPreset::Square));
        let mut page = Page::new("Page");
        page.layers.push(Layer::Text(TextLayer {
            common: LayerCommon {
                id: LayerId::new(),
                name: "T".to_string(),
                role: None,
                visible: true,
                locked: false,
                opacity: 1.0,
                transform: Transform::new(0.0, 0.0, 100.0, 100.0),
            },
            text: text.to_string(),
            font_family: "Geist".to_string(),
            font_size: 24.0,
            font_weight: 400,
            line_height: 1.2,
            letter_spacing: 0.0,
            align: TextAlign::Left,
            color: DesignColor::token("brand.text"),
        }));
        document.pages.push(page);
        document
    }

    /// A control character reached a real design through `apply_operations`,
    /// which accepted it, and `validate_design` then called that design valid
    /// while `preview.png` answered 500 SVG_PARSE_FAILED. The design could not be
    /// rendered, exported, or deleted -- there is no delete for designs.
    #[test]
    fn a_character_xml_cannot_carry_is_rejected_before_it_is_persisted() {
        let error = document_saying("hola\u{0}mundo")
            .validate_structure()
            .expect_err("a NUL must not enter a document");
        assert_eq!(error.code(), "INVALID_DOCUMENT");
        assert!(format!("{error}").contains("U+0000"), "{error}");
    }

    /// Line breaks are explicit newlines in V1 (plan section 18), so the legal
    /// XML whitespace has to keep working.
    #[test]
    fn tabs_and_newlines_are_still_allowed() {
        document_saying("dos\nlíneas\ty tabulador\r\n")
            .validate_structure()
            .expect("legal whitespace must pass");
    }
}
