use async_trait::async_trait;
use design_core::{AccountId, DesignDocument, ProjectId, Template, TemplateId, TemplateRole};
use sqlx::{Row, SqlitePool};

use crate::error::{PersistenceError, Result};
use crate::traits::TemplateRepository;

use super::{now, parse_uuid, OWNED_PROJECTS};

pub struct SqliteTemplateRepository {
    pool: SqlitePool,
}

impl SqliteTemplateRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

fn row_to_template(row: &sqlx::sqlite::SqliteRow) -> Result<Template> {
    let document: DesignDocument = serde_json::from_str(&row.get::<String, _>("document_json"))
        .map_err(|e| PersistenceError::CorruptDocument(format!("template document: {e}")))?;
    let roles: Vec<TemplateRole> = serde_json::from_str(&row.get::<String, _>("roles_json"))
        .map_err(|e| PersistenceError::CorruptDocument(format!("template roles: {e}")))?;

    Ok(Template {
        id: TemplateId(parse_uuid(row.get::<String, _>("id"))?),
        project_id: ProjectId(parse_uuid(row.get::<String, _>("project_id"))?),
        name: row.get("name"),
        description: row.get("description"),
        document,
        roles,
        created_at: row.get("created_at"),
    })
}

#[async_trait]
impl TemplateRepository for SqliteTemplateRepository {
    /// The row stamps its own `created_at`, so a caller cannot backdate one.
    async fn create(&self, account: AccountId, template: &Template) -> Result<Template> {
        super::owned_project(&self.pool, account, template.project_id).await?;

        let created_at = now();
        sqlx::query(
            "INSERT INTO templates (id, project_id, name, description, document_json, roles_json, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(template.id.to_string())
        .bind(template.project_id.to_string())
        .bind(&template.name)
        .bind(&template.description)
        .bind(serde_json::to_string(&template.document).map_err(|e| {
            PersistenceError::CorruptDocument(format!("template document: {e}"))
        })?)
        .bind(serde_json::to_string(&template.roles).map_err(|e| {
            PersistenceError::CorruptDocument(format!("template roles: {e}"))
        })?)
        .bind(&created_at)
        .execute(&self.pool)
        .await?;

        // Read it back rather than returning what was built in memory: a create
        // that trusts its own struct proves nothing about what was stored.
        self.get(account, template.id).await
    }

    async fn get(&self, account: AccountId, id: TemplateId) -> Result<Template> {
        let row = sqlx::query(&format!(
            "SELECT * FROM templates WHERE id = ? AND project_id IN {OWNED_PROJECTS}"
        ))
        .bind(id.to_string())
        .bind(account.to_string())
        .fetch_optional(&self.pool)
        .await?
        .ok_or(PersistenceError::NotFound(format!("template {id}")))?;
        row_to_template(&row)
    }

    async fn list_for_project(
        &self,
        account: AccountId,
        project_id: ProjectId,
    ) -> Result<Vec<Template>> {
        let rows = sqlx::query(&format!(
            "SELECT * FROM templates WHERE project_id = ? AND project_id IN {OWNED_PROJECTS}
                 ORDER BY created_at DESC, id DESC"
        ))
        .bind(project_id.to_string())
        .bind(account.to_string())
        .fetch_all(&self.pool)
        .await?;

        rows.iter().map(row_to_template).collect()
    }

    async fn delete(&self, account: AccountId, id: TemplateId) -> Result<()> {
        let deleted = sqlx::query(&format!(
            "DELETE FROM templates WHERE id = ? AND project_id IN {OWNED_PROJECTS}"
        ))
        .bind(id.to_string())
        .bind(account.to_string())
        .execute(&self.pool)
        .await?;

        if deleted.rows_affected() == 0 {
            return Err(PersistenceError::NotFound(format!("template {id}")));
        }
        Ok(())
    }
}
