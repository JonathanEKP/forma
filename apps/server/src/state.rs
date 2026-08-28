use std::sync::Arc;

use renderer::FontRegistry;
use sqlx::SqlitePool;

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub fonts: Arc<FontRegistry>,
}
