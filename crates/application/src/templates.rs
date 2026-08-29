//! Templates (plan section 85).
//!
//! A template is a design that came out well, kept so the next one can start
//! from its shape. What makes it more than a copy is the roles read out of it:
//! the agent sees what each slot is for and how much text the layout actually
//! held, which is a brief rather than a puzzle to reverse-engineer.

use design_core::{Actor, DesignId, DesignRecord, ProjectId, Template, TemplateDraft, TemplateId};
use persistence::{DesignRepository, TemplateRepository};

use crate::error::Result;

/// Keeps the shape of one design, at the version it is on now.
///
/// The roles are derived here rather than accepted from the caller: a brief that
/// disagrees with the document it describes is worse than none, and deriving
/// them means nobody has to author metadata by hand for a template to be useful.
pub async fn save_from_design(
    templates: &dyn TemplateRepository,
    designs: &dyn DesignRepository,
    design_id: DesignId,
    draft: &TemplateDraft,
) -> Result<Template> {
    let record = designs.get(design_id).await?;
    let roles = design_core::roles_of(&record.document);

    let template = Template {
        id: TemplateId::new(),
        project_id: record.project_id,
        name: draft.name.clone(),
        description: draft.description.clone(),
        document: record.document,
        roles,
        created_at: String::new(),
    };

    Ok(templates.create(&template).await?)
}

pub async fn list(
    templates: &dyn TemplateRepository,
    project_id: ProjectId,
) -> Result<Vec<Template>> {
    Ok(templates.list_for_project(project_id).await?)
}

pub async fn get(templates: &dyn TemplateRepository, id: TemplateId) -> Result<Template> {
    Ok(templates.get(id).await?)
}

pub async fn delete(templates: &dyn TemplateRepository, id: TemplateId) -> Result<()> {
    Ok(templates.delete(id).await?)
}

/// Starts a design from a template's shape, in the template's own project.
///
/// The copy carries the template's words, because a template is "make this kind
/// of piece again" rather than a blank layout: the agent replaces the copy it
/// wants to change and leaves the rest, and `sample_text` tells it what the
/// layout was drawn for.
pub async fn create_design(
    designs: &dyn DesignRepository,
    templates: &dyn TemplateRepository,
    template_id: TemplateId,
    name: &str,
    actor: &Actor,
) -> Result<DesignRecord> {
    let template = templates.get(template_id).await?;
    let document = template.instantiate();
    document.validate_structure()?;

    Ok(designs
        .create(template.project_id, name, &document, actor)
        .await?)
}
