use async_trait::async_trait;
use design_core::{AccountId, DesignId, Export, ExportId, ExportScope, PageId, Version};
use sqlx::{Row, SqlitePool};

use crate::error::{PersistenceError, Result};
use crate::traits::ExportRepository;

use super::{now, parse_uuid, OWNED_DESIGNS};

pub struct SqliteExportRepository {
    pool: SqlitePool,
}

impl SqliteExportRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

fn scope_str(scope: ExportScope) -> &'static str {
    match scope {
        ExportScope::SinglePage => "single_page",
        ExportScope::AllPages => "all_pages",
    }
}

fn scope_from(raw: &str) -> ExportScope {
    match raw {
        "all_pages" => ExportScope::AllPages,
        _ => ExportScope::SinglePage,
    }
}

fn row_to_export(row: &sqlx::sqlite::SqliteRow) -> Result<Export> {
    Ok(Export {
        id: ExportId(parse_uuid(row.get::<String, _>("id"))?),
        design_id: DesignId(parse_uuid(row.get::<String, _>("design_id"))?),
        design_version: Version(row.get::<i64, _>("design_version") as u64),
        scope: scope_from(&row.get::<String, _>("scope")),
        page_id: row
            .get::<Option<String>, _>("page_id")
            .map(parse_uuid)
            .transpose()?
            .map(PageId),
        scale: row.get::<f64, _>("scale") as f32,
        storage_key: row.get("storage_key"),
        byte_size: row.get("byte_size"),
        created_at: row.get("created_at"),
    })
}

const COLUMNS: &str = "id, design_id, design_version, scope, page_id, scale, storage_key, \
                       byte_size, created_at";

#[async_trait]
impl ExportRepository for SqliteExportRepository {
    async fn create(&self, account: AccountId, export: &Export) -> Result<Export> {
        super::owned_design(&self.pool, account, export.design_id).await?;

        // Stamped here rather than taken from the caller, like every other
        // repository. A caller that left the field empty would otherwise
        // persist a blank timestamp and only find out by reading the row back.
        let created_at = now();

        sqlx::query(
            "INSERT INTO exports (id, design_id, design_version, scope, page_id, scale,
                                  storage_key, byte_size, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(export.id.to_string())
        .bind(export.design_id.to_string())
        .bind(export.design_version.0 as i64)
        .bind(scope_str(export.scope))
        .bind(export.page_id.map(|id| id.to_string()))
        .bind(export.scale as f64)
        .bind(&export.storage_key)
        .bind(export.byte_size)
        .bind(&created_at)
        .execute(&self.pool)
        .await?;

        Ok(Export {
            created_at,
            ..export.clone()
        })
    }

    async fn get(&self, account: AccountId, id: ExportId) -> Result<Export> {
        let row = sqlx::query(&format!(
            "SELECT {COLUMNS} FROM exports WHERE id = ? AND design_id IN {OWNED_DESIGNS}"
        ))
        .bind(id.to_string())
        .bind(account.to_string())
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| PersistenceError::NotFound(format!("export {id}")))?;

        row_to_export(&row)
    }

    async fn list_for_design(
        &self,
        account: AccountId,
        design_id: DesignId,
    ) -> Result<Vec<Export>> {
        let rows = sqlx::query(&format!(
            "SELECT {COLUMNS} FROM exports
             WHERE design_id = ? AND design_id IN {OWNED_DESIGNS} ORDER BY created_at DESC"
        ))
        .bind(design_id.to_string())
        .bind(account.to_string())
        .fetch_all(&self.pool)
        .await?;

        rows.iter().map(row_to_export).collect()
    }
}
