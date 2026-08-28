//! The report shape of plan section 41.

use design_core::{LayerId, PageId};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::severity::Severity;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ValidationIssue {
    /// A stable code from plan section 42. Agents branch on this, not on the
    /// message, so it must never change meaning.
    pub code: String,
    pub severity: Severity,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_id: Option<PageId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer_id: Option<LayerId>,
    pub message: String,
    /// Machine-readable specifics, so a client can render a fix without parsing
    /// the message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(type = "Record<string, unknown> | null")]
    pub details: Option<serde_json::Value>,
}

impl ValidationIssue {
    pub fn new(code: &str, severity: Severity, message: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            severity,
            page_id: None,
            layer_id: None,
            message: message.into(),
            details: None,
        }
    }

    pub fn error(code: &str, message: impl Into<String>) -> Self {
        Self::new(code, Severity::Error, message)
    }

    pub fn warning(code: &str, message: impl Into<String>) -> Self {
        Self::new(code, Severity::Warning, message)
    }

    pub fn at_page(mut self, page_id: PageId) -> Self {
        self.page_id = Some(page_id);
        self
    }

    pub fn at_layer(mut self, page_id: PageId, layer_id: LayerId) -> Self {
        self.page_id = Some(page_id);
        self.layer_id = Some(layer_id);
        self
    }

    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ValidationReport {
    /// False when any issue blocks export.
    pub valid: bool,
    pub issues: Vec<ValidationIssue>,
}

impl ValidationReport {
    pub fn new(issues: Vec<ValidationIssue>) -> Self {
        let valid = !issues.iter().any(|i| i.severity.blocks_export());
        Self { valid, issues }
    }

    pub fn errors(&self) -> impl Iterator<Item = &ValidationIssue> {
        self.issues.iter().filter(|i| i.severity == Severity::Error)
    }

    pub fn warnings(&self) -> impl Iterator<Item = &ValidationIssue> {
        self.issues
            .iter()
            .filter(|i| i.severity == Severity::Warning)
    }

    pub fn has(&self, code: &str) -> bool {
        self.issues.iter().any(|i| i.code == code)
    }
}
