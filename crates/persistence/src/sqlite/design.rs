//! SQLite design repository, including the optimistic concurrency of section 27.

use async_trait::async_trait;
use design_core::{
    Actor, ActorType, DesignDocument, DesignId, DesignRecord, DesignSummary, ProjectId, Version,
    SCHEMA_VERSION,
};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::error::{PersistenceError, Result};
use crate::traits::{DesignRepository, VersionSummary};

use super::{now, parse_document, parse_uuid};

pub struct SqliteDesignRepository {
    pool: SqlitePool,
}

impl SqliteDesignRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

fn actor_type_str(actor: &Actor) -> &'static str {
    match actor.actor_type {
        ActorType::Human => "human",
        ActorType::Agent => "agent",
        ActorType::System => "system",
    }
}

fn actor_type_from(raw: &str) -> ActorType {
    match raw {
        "agent" => ActorType::Agent,
        "system" => ActorType::System,
        _ => ActorType::Human,
    }
}

#[async_trait]
impl DesignRepository for SqliteDesignRepository {
    async fn create(
        &self,
        project_id: ProjectId,
        name: &str,
        document: &DesignDocument,
        actor: &Actor,
    ) -> Result<DesignRecord> {
        document
            .validate_structure()
            .map_err(|e| PersistenceError::CorruptDocument(e.to_string()))?;

        let id = DesignId::new();
        let version = Version::INITIAL;
        let timestamp = now();
        let json = serde_json::to_string(document)
            .map_err(|e| PersistenceError::CorruptDocument(e.to_string()))?;

        // The row and its first snapshot are written together: a design must
        // never exist without the version it claims to be at.
        let mut tx = self.pool.begin().await?;

        sqlx::query(
            "INSERT INTO designs (id, project_id, name, schema_version, version, document_json, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id.to_string())
        .bind(project_id.to_string())
        .bind(name)
        .bind(SCHEMA_VERSION)
        .bind(version.0 as i64)
        .bind(&json)
        .bind(&timestamp)
        .bind(&timestamp)
        .execute(&mut *tx)
        .await
        .map_err(|e| match &e {
            sqlx::Error::Database(db) if db.message().contains("FOREIGN KEY") => {
                PersistenceError::ProjectNotFound(project_id)
            }
            _ => e.into(),
        })?;

        insert_snapshot(&mut tx, id, version, &json, actor, &timestamp).await?;
        tx.commit().await?;

        Ok(DesignRecord {
            id,
            project_id,
            name: name.to_string(),
            version,
            created_at: timestamp.clone(),
            updated_at: timestamp,
            document: document.clone(),
        })
    }

    async fn get(&self, id: DesignId) -> Result<DesignRecord> {
        let row = sqlx::query(
            "SELECT id, project_id, name, version, document_json, created_at, updated_at
             FROM designs WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await?
        .ok_or(PersistenceError::DesignNotFound(id))?;

        Ok(DesignRecord {
            id,
            project_id: ProjectId(parse_uuid(row.get::<String, _>("project_id"))?),
            name: row.get("name"),
            version: Version(row.get::<i64, _>("version") as u64),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
            document: parse_document(row.get::<String, _>("document_json"))?,
        })
    }

    async fn list_for_project(&self, project_id: ProjectId) -> Result<Vec<DesignSummary>> {
        let rows = sqlx::query(
            "SELECT id, name, version, document_json, created_at, updated_at
             FROM designs WHERE project_id = ? ORDER BY updated_at DESC",
        )
        .bind(project_id.to_string())
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(|row| {
                let document: DesignDocument =
                    parse_document(row.get::<String, _>("document_json"))?;
                Ok(DesignSummary {
                    id: DesignId(parse_uuid(row.get::<String, _>("id"))?),
                    project_id,
                    name: row.get("name"),
                    version: Version(row.get::<i64, _>("version") as u64),
                    format: document.format,
                    page_count: document.pages.len(),
                    created_at: row.get("created_at"),
                    updated_at: row.get("updated_at"),
                })
            })
            .collect()
    }

    async fn save(
        &self,
        id: DesignId,
        base_version: Version,
        document: &DesignDocument,
        actor: &Actor,
    ) -> Result<DesignRecord> {
        document
            .validate_structure()
            .map_err(|e| PersistenceError::CorruptDocument(e.to_string()))?;

        let next = base_version.next();
        let timestamp = now();
        let json = serde_json::to_string(document)
            .map_err(|e| PersistenceError::CorruptDocument(e.to_string()))?;

        let mut tx = self.pool.begin().await?;

        // The whole concurrency story is this WHERE clause: the update only
        // lands if nobody else moved the design since we read it.
        let updated = sqlx::query(
            "UPDATE designs
             SET version = ?, document_json = ?, updated_at = ?
             WHERE id = ? AND version = ?",
        )
        .bind(next.0 as i64)
        .bind(&json)
        .bind(&timestamp)
        .bind(id.to_string())
        .bind(base_version.0 as i64)
        .execute(&mut *tx)
        .await?;

        if updated.rows_affected() == 0 {
            // Zero rows is ambiguous on its own: either the design is gone or
            // the version moved. Tell the caller which, so it can act.
            let actual: Option<i64> =
                sqlx::query_scalar("SELECT version FROM designs WHERE id = ?")
                    .bind(id.to_string())
                    .fetch_optional(&mut *tx)
                    .await?;

            return Err(match actual {
                Some(actual) => PersistenceError::VersionConflict {
                    expected: base_version,
                    actual: Version(actual as u64),
                },
                None => PersistenceError::DesignNotFound(id),
            });
        }

        insert_snapshot(&mut tx, id, next, &json, actor, &timestamp).await?;
        tx.commit().await?;

        self.get(id).await
    }

    async fn history(&self, id: DesignId) -> Result<Vec<VersionSummary>> {
        let rows = sqlx::query(
            "SELECT version, actor_type, actor_id, created_at
             FROM design_versions WHERE design_id = ? ORDER BY version DESC",
        )
        .bind(id.to_string())
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| VersionSummary {
                version: Version(row.get::<i64, _>("version") as u64),
                actor_type: actor_type_from(&row.get::<String, _>("actor_type")),
                actor_id: row.get("actor_id"),
                created_at: row.get("created_at"),
            })
            .collect())
    }

    async fn snapshot(&self, id: DesignId, version: Version) -> Result<DesignDocument> {
        let json: Option<String> = sqlx::query_scalar(
            "SELECT document_json FROM design_versions WHERE design_id = ? AND version = ?",
        )
        .bind(id.to_string())
        .bind(version.0 as i64)
        .fetch_optional(&self.pool)
        .await?;

        match json {
            Some(json) => parse_document(json),
            None => Err(PersistenceError::NotFound(format!(
                "version {version} of design {id}"
            ))),
        }
    }

    async fn all_ids(&self) -> Result<Vec<DesignId>> {
        let rows: Vec<String> = sqlx::query_scalar("SELECT id FROM designs")
            .fetch_all(&self.pool)
            .await?;

        rows.into_iter()
            .map(|raw| Ok(DesignId(parse_uuid(raw)?)))
            .collect()
    }

    async fn delete(&self, id: DesignId) -> Result<()> {
        let deleted = sqlx::query("DELETE FROM designs WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;

        if deleted.rows_affected() == 0 {
            return Err(PersistenceError::DesignNotFound(id));
        }
        Ok(())
    }
}

async fn insert_snapshot(
    tx: &mut sqlx::SqliteConnection,
    design_id: DesignId,
    version: Version,
    json: &str,
    actor: &Actor,
    timestamp: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO design_versions (id, design_id, version, document_json, actor_type, actor_id, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(design_id.to_string())
    .bind(version.0 as i64)
    .bind(json)
    .bind(actor_type_str(actor))
    .bind(actor.actor_id.as_deref())
    .bind(timestamp)
    .execute(tx)
    .await?;
    Ok(())
}
