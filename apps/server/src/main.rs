use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use actix_web::{web, App, HttpServer};
use forma_server::{auth, backup, routes, telemetry, AppState, Config};
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

    // One subcommand, deliberately: a backup is the one thing you need the
    // server's own configuration for but do not want a running server to do.
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        None => {}
        Some("backup") => {
            let out = args
                .next()
                .map(PathBuf::from)
                .unwrap_or_else(|| backup::default_destination(&config.data_dir));

            let summary = backup::run(&config, &out).await?;
            tracing::info!(
                out = %summary.out.display(),
                database_bytes = summary.database_bytes,
                asset_files = summary.asset_files,
                asset_bytes = summary.asset_bytes,
                "backup written"
            );
            println!("{}", summary.out.display());
            return Ok(());
        }
        Some("--help" | "-h" | "help") => {
            println!("forma-server                 run the server");
            println!("forma-server backup [DIR]    write a backup, default DATA_DIR/backups/<utc>");
            return Ok(());
        }
        Some(other) => anyhow::bail!("unknown argument {other:?}; try --help"),
    }

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

    // Read the account this process serves, once, rather than trusting the
    // constant the migration writes. A database whose migrations never ran fails
    // here, with a message, instead of on somebody's first request.
    let account = persistence::AccountRepository::bootstrap(&*state.accounts)
        .await
        .map_err(|e| std::io::Error::other(format!("no account to serve: {e}")))?;
    tracing::info!(account = %account.id, claimed = account.claimed_at.is_some(), "serving one account");
    let state = state.with_account(account.id);

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

    // Kept out of the closure, which consumes the state, so the pool can still
    // be closed after the server has stopped.
    let pool = state.pool.clone();
    let web_dir = config.web_dir.clone();
    let bind = config.bind.clone();

    // The one configuration that must not start: reachable from the network
    // with nothing in front of it.
    if let Err(reason) = config.binding_is_safe() {
        tracing::error!("{reason}");
        anyhow::bail!(reason);
    }

    tracing::info!(
        bind = %bind,
        authenticated = config.remote_token.is_some(),
        "forma-server listening"
    );

    let token = config.remote_token.as_deref().map(auth::RemoteToken::new);

    let server = HttpServer::new(move || {
        let mut app = App::new()
            // Auth wraps outside tracing so an unauthorized call is still one
            // logged request, with its own status rather than silence.
            .wrap(actix_web::middleware::from_fn(auth::require_bearer))
            .wrap(actix_web::middleware::from_fn(telemetry::trace_request))
            .app_data(web::Data::new(state.clone()));

        if let Some(token) = token.clone() {
            app = app.app_data(web::Data::new(token));
        }

        let app = app.configure(routes::configure);

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
    // How long in-flight work gets to finish before it is dropped.
    //
    // Measured: a single Story page at scale 4 — the largest canvas this
    // server renders — takes about 8.6 seconds. Twenty leaves room for a
    // slower machine while still being a wait a person will sit through.
    //
    // Exporting many pages at that scale can exceed it and be cut off. That is
    // safe rather than merely tolerable: an export writes its files before the
    // row that names them, so a cut one leaves files nothing points at, never
    // a record pointing at files that were never written.
    .shutdown_timeout(20)
    // Handled below instead, so the log says what is happening rather than the
    // process simply vanishing.
    .disable_signals()
    .run();

    let stopper = server.handle();
    actix_web::rt::spawn(async move {
        wait_for_signal().await;
        tracing::info!("stopping: finishing in-flight requests");
        // Graceful: stop accepting, let what is running finish.
        stopper.stop(true).await;
    });

    server.await?;

    // Folds the write-ahead log back into the database file and closes. Doing
    // it explicitly rather than relying on the close to trigger it: the close
    // finishes on sqlx's workers, so the checkpoint could land after we have
    // said we stopped, and a backup taken right then would copy a file with
    // none of the writes in it.
    persistence::checkpoint_and_close(&pool).await?;

    tracing::info!("stopped cleanly");
    Ok(())
}

/// Resolves on the first signal asking the process to stop.
async fn wait_for_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};

        let mut terminate = match signal(SignalKind::terminate()) {
            Ok(stream) => stream,
            Err(error) => {
                tracing::warn!(%error, "cannot listen for SIGTERM; Ctrl-C only");
                let _ = tokio::signal::ctrl_c().await;
                return;
            }
        };

        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = terminate.recv() => {}
        }
    }

    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
