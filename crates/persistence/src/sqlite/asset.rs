use async_trait::async_trait;
use design_core::{AccountId, Asset, AssetId, AssetKind, AssetOrigin, ProjectId};
use sqlx::{Row, SqlitePool};

use crate::error::{PersistenceError, Result};
use crate::traits::AssetRepository;

use super::{now, parse_uuid, OWNED_PROJECTS};

pub struct SqliteAssetRepository {
    pool: SqlitePool,
}

impl SqliteAssetRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

fn kind_str(kind: AssetKind) -> &'static str {
    match kind {
        AssetKind::Raster => "raster",
        AssetKind::Vector => "vector",
    }
}

fn kind_from(raw: &str) -> AssetKind {
    match raw {
        "vector" => AssetKind::Vector,
        _ => AssetKind::Raster,
    }
}

fn origin_str(origin: AssetOrigin) -> &'static str {
    match origin {
        AssetOrigin::UserUpload => "user_upload",
        AssetOrigin::RemoteUrl => "remote_url",
        AssetOrigin::HostGenerated => "host_generated",
        AssetOrigin::Stock => "stock",
    }
}

fn origin_from(raw: &str) -> AssetOrigin {
    match raw {
        "remote_url" => AssetOrigin::RemoteUrl,
        "host_generated" => AssetOrigin::HostGenerated,
        "stock" => AssetOrigin::Stock,
        _ => AssetOrigin::UserUpload,
    }
}

fn row_to_asset(row: &sqlx::sqlite::SqliteRow) -> Result<Asset> {
    Ok(Asset {
        id: AssetId(parse_uuid(row.get::<String, _>("id"))?),
        project_id: ProjectId(parse_uuid(row.get::<String, _>("project_id"))?),
        hash: row.get("hash"),
        kind: kind_from(&row.get::<String, _>("kind")),
        mime_type: row.get("mime_type"),
        width: row.get::<i64, _>("width") as u32,
        height: row.get::<i64, _>("height") as u32,
        byte_size: row.get("byte_size"),
        storage_key: row.get("storage_key"),
        original_filename: row.get("original_filename"),
        origin: origin_from(&row.get::<String, _>("origin")),
        attribution: row.get("attribution"),
        created_at: row.get("created_at"),
    })
}

const COLUMNS: &str = "id, project_id, hash, kind, mime_type, width, height, byte_size, \
                       storage_key, original_filename, origin, attribution, created_at";

#[async_trait]
impl AssetRepository for SqliteAssetRepository {
    async fn create(&self, account: AccountId, asset: &Asset) -> Result<Asset> {
        super::owned_project(&self.pool, account, asset.project_id).await?;

        // Stamped here rather than taken from the caller, matching the design
        // repository. A caller that forgot the field would otherwise write an
        // empty timestamp, and one that did not forget could backdate a row.
        let created_at = now();

        sqlx::query(
            "INSERT INTO assets (id, project_id, hash, kind, mime_type, width, height, byte_size,
                                 storage_key, original_filename, origin, attribution, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(asset.id.to_string())
        .bind(asset.project_id.to_string())
        .bind(&asset.hash)
        .bind(kind_str(asset.kind))
        .bind(&asset.mime_type)
        .bind(asset.width as i64)
        .bind(asset.height as i64)
        .bind(asset.byte_size)
        .bind(&asset.storage_key)
        .bind(asset.original_filename.as_deref())
        .bind(origin_str(asset.origin))
        .bind(asset.attribution.as_deref())
        .bind(&created_at)
        .execute(&self.pool)
        .await?;

        Ok(Asset {
            created_at,
            ..asset.clone()
        })
    }

    async fn get(&self, account: AccountId, id: AssetId) -> Result<Asset> {
        let row = sqlx::query(&format!(
            "SELECT {COLUMNS} FROM assets
             WHERE id = ? AND deleted_at IS NULL AND project_id IN {OWNED_PROJECTS}"
        ))
        .bind(id.to_string())
        .bind(account.to_string())
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| PersistenceError::NotFound(format!("asset {id}")))?;

        row_to_asset(&row)
    }

    async fn list_for_project(
        &self,
        account: AccountId,
        project_id: ProjectId,
    ) -> Result<Vec<Asset>> {
        let rows = sqlx::query(&format!(
            "SELECT {COLUMNS} FROM assets
             WHERE project_id = ? AND project_id IN {OWNED_PROJECTS} AND deleted_at IS NULL
             ORDER BY created_at DESC"
        ))
        .bind(project_id.to_string())
        .bind(account.to_string())
        .fetch_all(&self.pool)
        .await?;

        rows.iter().map(row_to_asset).collect()
    }

    async fn find_by_hash(
        &self,
        account: AccountId,
        project_id: ProjectId,
        hash: &str,
    ) -> Result<Option<Asset>> {
        let row = sqlx::query(&format!(
            "SELECT {COLUMNS} FROM assets
             WHERE project_id = ? AND hash = ? AND deleted_at IS NULL
               AND project_id IN {OWNED_PROJECTS}"
        ))
        .bind(project_id.to_string())
        .bind(hash)
        .bind(account.to_string())
        .fetch_optional(&self.pool)
        .await?;

        row.as_ref().map(row_to_asset).transpose()
    }

    async fn soft_delete(&self, account: AccountId, id: AssetId) -> Result<()> {
        // The bytes stay: historical versions may still reference them.
        let updated = sqlx::query(&format!(
            "UPDATE assets SET deleted_at = ?
                 WHERE id = ? AND deleted_at IS NULL AND project_id IN {OWNED_PROJECTS}"
        ))
        .bind(now())
        .bind(id.to_string())
        .bind(account.to_string())
        .execute(&self.pool)
        .await?;

        if updated.rows_affected() == 0 {
            return Err(PersistenceError::NotFound(format!("asset {id}")));
        }
        Ok(())
    }
}
