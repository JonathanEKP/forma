use std::collections::HashSet;
use std::sync::Arc;

use actix_web::{web, App, HttpServer};
use forma_server::{routes, telemetry, AppState, Config};
use persistence::DesignRepository;
use renderer::{FontRegistry, RenderCache};
use tracing_subscriber::EnvFilter;

/// Drops cached renders for designs that are gone.
async fn sweep_render_cache(state: &AppState) -> anyhow::Result<renderer::cache::Pruned> {
    let live: HashSet<_> = state.designs.all_ids().await?.into_iter().collect();
    Ok(state.cache.sweep(&live)?)
}

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    let config = Config::from_env();

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new(&config.log))
        .with_target(false)
        .init();

    let fonts = Arc::new(FontRegistry::from_dir(&config.font_dir)?);
    tracing::info!(
        font_dir = %config.font_dir.display(),
        families = fonts.families().len(),
        "font registry loaded"
    );

    let pool = persistence::connect(&config.database_url, &config.data_dir).await?;
    tracing::info!(database_url = %config.database_url, "migrations applied");

    let cache = Arc::new(RenderCache::new(config.data_dir.join("previews")));
    let store = Arc::new(persistence::LocalObjectStore::new(&config.data_dir));
    let state = AppState::new(pool, fonts, cache, store);

    // Renders of designs that no longer exist (plan section 74). Doing it at
    // startup rather than on every delete path means a path added later cannot
    // forget, and anything a crash left behind is collected too.
    match sweep_render_cache(&state).await {
        Ok(pruned) if pruned.files > 0 => tracing::info!(
            files = pruned.files,
            bytes = pruned.bytes,
            "swept renders of deleted designs"
        ),
        Err(error) => tracing::warn!(%error, "could not sweep the render cache"),
        _ => {}
    }

    let web_dir = config.web_dir.clone();
    let bind = config.bind.clone();

    tracing::info!(bind = %bind, "forma-server listening");

    HttpServer::new(move || {
        let app = App::new()
            .wrap(actix_web::middleware::from_fn(telemetry::trace_request))
            .app_data(web::Data::new(state.clone()))
            .configure(routes::configure);

        // The built React app is served from the same origin, so the whole
        // product is one URL (plan section 58).
        if web_dir.is_dir() {
            app.service(
                actix_files::Files::new("/", &web_dir)
                    .index_file("index.html")
                    .prefer_utf8(true),
            )
        } else {
            app
        }
    })
    .bind(&bind)?
    .run()
    .await?;

    Ok(())
}
