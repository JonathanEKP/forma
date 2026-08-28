use std::sync::Arc;

use persistence::{
    LocalObjectStore, SqliteAssetRepository, SqliteAssetRequestRepository, SqliteDesignRepository,
    SqliteExportRepository, SqliteProjectRepository,
};
use renderer::{FontRegistry, RenderCache};
use sqlx::SqlitePool;

/// Everything a request handler needs.
///
/// The repositories are concrete here rather than boxed trait objects: there is
/// exactly one implementation today, and plan section 24 warns against building
/// the abstraction before a second one exists. The traits are what the deploy
/// phase will swap, and the use cases already take `&dyn`, so that swap does not
/// reach the handlers.
#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub fonts: Arc<FontRegistry>,
    pub cache: Arc<RenderCache>,
    pub store: Arc<LocalObjectStore>,
    pub projects: Arc<SqliteProjectRepository>,
    pub designs: Arc<SqliteDesignRepository>,
    pub assets: Arc<SqliteAssetRepository>,
    pub asset_requests: Arc<SqliteAssetRequestRepository>,
    pub exports: Arc<SqliteExportRepository>,
}

impl AppState {
    pub fn new(
        pool: SqlitePool,
        fonts: Arc<FontRegistry>,
        cache: Arc<RenderCache>,
        store: Arc<LocalObjectStore>,
    ) -> Self {
        Self {
            projects: Arc::new(SqliteProjectRepository::new(pool.clone())),
            designs: Arc::new(SqliteDesignRepository::new(pool.clone())),
            assets: Arc::new(SqliteAssetRepository::new(pool.clone())),
            asset_requests: Arc::new(SqliteAssetRequestRepository::new(pool.clone())),
            exports: Arc::new(SqliteExportRepository::new(pool.clone())),
            pool,
            fonts,
            cache,
            store,
        }
    }
}
