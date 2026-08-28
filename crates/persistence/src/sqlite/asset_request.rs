use async_trait::async_trait;
use design_core::{AssetId, AssetRequest, AssetRequestId, AssetRequestStatus, DesignId, ProjectId};
use sqlx::{Row, SqlitePool};

use crate::error::{PersistenceError, Result};
use crate::traits::AssetRequestRepository;

use super::{now, parse_uuid};

pub struct SqliteAssetRequestRepository {
    pool: SqlitePool,
}

impl SqliteAssetRequestRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

fn status_str(status: AssetRequestStatus) -> &'static str {
    match status {
        AssetRequestStatus::Pending => "pending",
        AssetRequestStatus::Resolved => "resolved",
        AssetRequestStatus::Cancelled => "cancelled",
    }
}

fn status_from(raw: &str) -> AssetRequestStatus {
    match raw {
        "resolved" => AssetRequestStatus::Resolved,
        "cancelled" => AssetRequestStatus::Cancelled,
        _ => AssetRequestStatus::Pending,
    }
}

fn row_to_request(row: &sqlx::sqlite::SqliteRow) -> Result<AssetRequest> {
    Ok(AssetRequest {
        id: AssetRequestId(parse_uuid(row.get::<String, _>("id"))?),
        project_id: ProjectId(parse_uuid(row.get::<String, _>("project_id"))?),
        design_id: row
            .get::<Option<String>, _>("design_id")
            .map(parse_uuid)
            .transpose()?
            .map(DesignId),
        purpose: row.get("purpose"),
        description: row.get("description"),
        suggested_prompt: row.get("suggested_prompt"),
        requested_width: row
            .get::<Option<i64>, _>("requested_width")
            .map(|v| v as u32),
        requested_height: row
            .get::<Option<i64>, _>("requested_height")
            .map(|v| v as u32),
        status: status_from(&row.get::<String, _>("status")),
        resolved_asset_id: row
            .get::<Option<String>, _>("resolved_asset_id")
            .map(parse_uuid)
            .transpose()?
            .map(AssetId),
        created_at: row.get("created_at"),
        resolved_at: row.get("resolved_at"),
    })
}

const COLUMNS: &str = "id, project_id, design_id, purpose, description, suggested_prompt, \
                       requested_width, requested_height, status, resolved_asset_id, \
                       created_at, resolved_at";

#[async_trait]
impl AssetRequestRepository for SqliteAssetRequestRepository {
    async fn create(&self, request: &AssetRequest) -> Result<AssetRequest> {
        sqlx::query(
            "INSERT INTO asset_requests (id, project_id, design_id, purpose, description,
                                         suggested_prompt, requested_width, requested_height,
                                         status, resolved_asset_id, created_at, resolved_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(request.id.to_string())
        .bind(request.project_id.to_string())
        .bind(request.design_id.map(|id| id.to_string()))
        .bind(&request.purpose)
        .bind(&request.description)
        .bind(request.suggested_prompt.as_deref())
        .bind(request.requested_width.map(|v| v as i64))
        .bind(request.requested_height.map(|v| v as i64))
        .bind(status_str(request.status))
        .bind(request.resolved_asset_id.map(|id| id.to_string()))
        .bind(&request.created_at)
        .bind(request.resolved_at.as_deref())
        .execute(&self.pool)
        .await?;

        Ok(request.clone())
    }

    async fn get(&self, id: AssetRequestId) -> Result<AssetRequest> {
        let row = sqlx::query(&format!(
            "SELECT {COLUMNS} FROM asset_requests WHERE id = ?"
        ))
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| PersistenceError::NotFound(format!("asset request {id}")))?;

        row_to_request(&row)
    }

    async fn list_for_project(&self, project_id: ProjectId) -> Result<Vec<AssetRequest>> {
        let rows = sqlx::query(&format!(
            "SELECT {COLUMNS} FROM asset_requests
             WHERE project_id = ? ORDER BY created_at DESC"
        ))
        .bind(project_id.to_string())
        .fetch_all(&self.pool)
        .await?;

        rows.iter().map(row_to_request).collect()
    }

    async fn resolve(&self, id: AssetRequestId, asset_id: AssetId) -> Result<AssetRequest> {
        // Only a pending request can be resolved, so a late second upload cannot
        // silently overwrite the asset the design already uses.
        let updated = sqlx::query(
            "UPDATE asset_requests
             SET status = 'resolved', resolved_asset_id = ?, resolved_at = ?
             WHERE id = ? AND status = 'pending'",
        )
        .bind(asset_id.to_string())
        .bind(now())
        .bind(id.to_string())
        .execute(&self.pool)
        .await?;

        if updated.rows_affected() == 0 {
            let existing = self.get(id).await?;
            return Err(PersistenceError::Storage(format!(
                "asset request {id} is already {:?} and cannot be resolved again",
                existing.status
            )));
        }
        self.get(id).await
    }

    async fn cancel(&self, id: AssetRequestId) -> Result<AssetRequest> {
        let updated = sqlx::query(
            "UPDATE asset_requests SET status = 'cancelled' WHERE id = ? AND status = 'pending'",
        )
        .bind(id.to_string())
        .execute(&self.pool)
        .await?;

        if updated.rows_affected() == 0 {
            let existing = self.get(id).await?;
            return Err(PersistenceError::Storage(format!(
                "asset request {id} is already {:?}",
                existing.status
            )));
        }
        self.get(id).await
    }
}
