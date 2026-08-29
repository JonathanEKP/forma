//! Templates (plan section 85).
//!
//! A template is a `DesignDocument` plus role metadata. The metadata is the
//! point: without it a template is a document you copied, and the agent has to
//! work out what each layer is for by reading coordinates. With it, the template
//! states its own brief — this page wants a headline of about this length, and
//! here is what was in it when the layout worked.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::document::DesignDocument;
use crate::ids::{ProjectId, TemplateId};
use crate::layer::LayerRole;

/// One slot the template expects to be filled.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TemplateRole {
    pub page_index: usize,
    pub role: LayerRole,
    /// How much text the layout actually held. Derived from the design it was
    /// saved from rather than typed by hand: the number is only honest if it
    /// describes a layout that really fitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_chars: Option<usize>,
    /// What was in the slot when it was saved, so the agent can see the register
    /// the layout was drawn for rather than guess it from a length.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sample_text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Template {
    pub id: TemplateId,
    pub project_id: ProjectId,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub document: DesignDocument,
    pub roles: Vec<TemplateRole>,
    pub created_at: String,
}

/// What a caller may say when saving one. The id and the timestamp are the
/// server's, and the roles are derived rather than supplied — a brief that
/// disagrees with the document it describes is worse than none.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TemplateDraft {
    pub name: String,
    #[serde(default)]
    pub description: String,
}

impl Template {
    /// A copy of the stored document with fresh identity.
    ///
    /// Page and layer ids are minted again so two designs made from one template
    /// never share them. The brief survives that because section 85 addresses
    /// slots by `page_index` and role rather than by id — which is the reason it
    /// was specified that way.
    pub fn instantiate(&self) -> DesignDocument {
        let mut document = self.document.clone();
        for page in &mut document.pages {
            page.id = crate::ids::PageId::new();
            for layer in &mut page.layers {
                layer.common_mut().id = crate::ids::LayerId::new();
            }
        }
        document
    }
}

/// Reads the brief out of a document.
///
/// Only layers that carry a `role` become slots: a role is the author saying
/// what a layer is for, and a template of unlabelled boxes teaches nothing.
pub fn roles_of(document: &DesignDocument) -> Vec<TemplateRole> {
    let mut roles = Vec::new();
    for (page_index, page) in document.pages.iter().enumerate() {
        for layer in &page.layers {
            let Some(role) = layer.common().role else {
                continue;
            };
            let text = match layer {
                crate::layer::Layer::Text(text) => Some(text.text.clone()),
                _ => None,
            };
            roles.push(TemplateRole {
                page_index,
                role,
                max_chars: text.as_ref().map(|t| t.chars().count()),
                sample_text: text,
            });
        }
    }
    roles
}
