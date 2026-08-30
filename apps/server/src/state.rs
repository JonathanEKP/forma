use std::sync::Arc;

use design_core::AccountId;

use persistence::{
    LocalObjectStore, SqliteAccountRepository, SqliteAssetRepository, SqliteAssetRequestRepository, SqliteDesignRepository,
    SqliteExportRepository, SqliteProjectRepository, SqliteTemplateRepository,
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
    /// Plan section 44: the upload ceiling, enforced while reading the body.
    pub max_upload_bytes: usize,
    pub projects: Arc<SqliteProjectRepository>,
    pub designs: Arc<SqliteDesignRepository>,
    pub assets: Arc<SqliteAssetRepository>,
    pub asset_requests: Arc<SqliteAssetRequestRepository>,
    pub exports: Arc<SqliteExportRepository>,
    pub templates: Arc<SqliteTemplateRepository>,
    pub accounts: Arc<SqliteAccountRepository>,
    /// The account every request in this process resolves to.
    ///
    /// Slice one is local mode (plan 117.8): one account, no identity, and the
    /// security boundary is the process. It is a field rather than a lookup per
    /// request because the answer cannot change while the process runs — and it
    /// is the seam the hosted slice replaces, when the account starts coming
    /// from a credential instead.
    pub account: AccountId,
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
            templates: Arc::new(SqliteTemplateRepository::new(pool.clone())),
            accounts: Arc::new(SqliteAccountRepository::new(pool.clone())),
            account: AccountId(
                uuid::Uuid::parse_str(design_core::BOOTSTRAP_ACCOUNT).expect("a constant uuid"),
            ),
            pool,
            fonts,
            cache,
            store,
            max_upload_bytes: assets::DEFAULT_MAX_UPLOAD_BYTES,
        }
    }

    /// Overrides the upload ceiling, so a deployment can tighten it without
    /// recompiling (plan section 59).
    pub fn with_max_upload_bytes(mut self, max: usize) -> Self {
        self.max_upload_bytes = max;
        self
    }

    /// Points this process at a different account.
    ///
    /// `new` defaults to the bootstrap account of the migration, which is what
    /// local mode wants and what every test wants. Startup overrides it with the
    /// row it actually read, so a database whose migrations never ran fails
    /// there rather than on the first request.
    pub fn with_account(mut self, account: AccountId) -> Self {
        self.account = account;
        self
    }
}
