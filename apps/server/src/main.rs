use std::sync::Arc;

use actix_web::{web, App, HttpServer};
use forma_server::{db, routes, AppState, Config};
use renderer::FontRegistry;
use tracing_subscriber::EnvFilter;

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

    let pool = db::connect(&config.database_url, &config.data_dir).await?;
    tracing::info!(database_url = %config.database_url, "migrations applied");

    let state = AppState { pool, fonts };
    let web_dir = config.web_dir.clone();
    let bind = config.bind.clone();

    tracing::info!(bind = %bind, "forma-server listening");

    HttpServer::new(move || {
        let app = App::new()
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
