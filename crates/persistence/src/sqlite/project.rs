use async_trait::async_trait;
use design_core::{BrandKit, Project, ProjectId};
use sqlx::{Row, SqlitePool};

use crate::error::{PersistenceError, Result};
use crate::traits::ProjectRepository;

use super::{now, parse_uuid};

pub struct SqliteProjectRepository {
    pool: SqlitePool,
}

impl SqliteProjectRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ProjectRepository for SqliteProjectRepository {
    async fn create(&self, name: &str) -> Result<Project> {
        let project = Project {
            id: ProjectId::new(),
            name: name.to_string(),
            brand_id: None,
            created_at: now(),
            updated_at: now(),
        };

        sqlx::query(
            "INSERT INTO projects (id, name, brand_id, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(project.id.to_string())
        .bind(&project.name)
        .bind(Option::<String>::None)
        .bind(&project.created_at)
        .bind(&project.updated_at)
        .execute(&self.pool)
        .await?;

        Ok(project)
    }

    async fn get(&self, id: ProjectId) -> Result<Project> {
        let row = sqlx::query(
            "SELECT id, name, brand_id, created_at, updated_at FROM projects WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await?
        .ok_or(PersistenceError::ProjectNotFound(id))?;

        Ok(Project {
            id,
            name: row.get("name"),
            brand_id: row
                .get::<Option<String>, _>("brand_id")
                .map(parse_uuid)
                .transpose()?
                .map(design_core::BrandId),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        })
    }

    async fn list(&self) -> Result<Vec<Project>> {
        let rows = sqlx::query(
            "SELECT id, name, brand_id, created_at, updated_at
             FROM projects ORDER BY updated_at DESC",
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(|row| {
                Ok(Project {
                    id: ProjectId(parse_uuid(row.get::<String, _>("id"))?),
                    name: row.get("name"),
                    brand_id: row
                        .get::<Option<String>, _>("brand_id")
                        .map(parse_uuid)
                        .transpose()?
                        .map(design_core::BrandId),
                    created_at: row.get("created_at"),
                    updated_at: row.get("updated_at"),
                })
            })
            .collect()
    }

    async fn rename(&self, id: ProjectId, name: &str) -> Result<Project> {
        let updated = sqlx::query("UPDATE projects SET name = ?, updated_at = ? WHERE id = ?")
            .bind(name)
            .bind(now())
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;

        if updated.rows_affected() == 0 {
            return Err(PersistenceError::ProjectNotFound(id));
        }
        self.get(id).await
    }

    async fn delete(&self, id: ProjectId) -> Result<()> {
        let deleted = sqlx::query("DELETE FROM projects WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;

        if deleted.rows_affected() == 0 {
            return Err(PersistenceError::ProjectNotFound(id));
        }
        Ok(())
    }

    async fn get_brand(&self, id: ProjectId) -> Result<Option<BrandKit>> {
        let json: Option<String> =
            sqlx::query_scalar("SELECT brand_json FROM brand_kits WHERE project_id = ?")
                .bind(id.to_string())
                .fetch_optional(&self.pool)
                .await?;

        json.map(|json| {
            serde_json::from_str(&json)
                .map_err(|e| PersistenceError::CorruptDocument(e.to_string()))
        })
        .transpose()
    }

    async fn put_brand(&self, brand: &BrandKit) -> Result<BrandKit> {
        let json = serde_json::to_string(brand)
            .map_err(|e| PersistenceError::CorruptDocument(e.to_string()))?;
        let timestamp = now();

        // One active Brand Kit per project (plan section 12), so the project id
        // is the conflict target rather than the brand id.
        sqlx::query(
            "INSERT INTO brand_kits (id, project_id, name, brand_json, version, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT (project_id) DO UPDATE SET
                 id = excluded.id,
                 name = excluded.name,
                 brand_json = excluded.brand_json,
                 version = excluded.version,
                 updated_at = excluded.updated_at",
        )
        .bind(brand.id.to_string())
        .bind(brand.project_id.to_string())
        .bind(&brand.name)
        .bind(&json)
        .bind(brand.version.0 as i64)
        .bind(&timestamp)
        .bind(&timestamp)
        .execute(&self.pool)
        .await?;

        sqlx::query("UPDATE projects SET brand_id = ?, updated_at = ? WHERE id = ?")
            .bind(brand.id.to_string())
            .bind(&timestamp)
            .bind(brand.project_id.to_string())
            .execute(&self.pool)
            .await?;

        Ok(brand.clone())
    }
}
