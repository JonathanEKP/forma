use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use actix_web::{dev::fn_service, http::header, middleware::DefaultHeaders, web, App, HttpServer};
use anyhow::Context;
use design_core::{AccountRole, AuthUserId};
use forma_server::{auth, backup, routes, telemetry, AppState, Config};
use persistence::ApiKeyRepository;
use persistence::{AccountRepository, DesignRepository};
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

    // These commands use the server's own configuration while keeping shell
    // access as their security boundary. None belongs on the network API: a
    // backup touches the whole workspace, and invites and recovery grants are
    // decisions only the machine's operator may make.
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
        Some("invite") => {
            let email = required_email(args.next(), "forma-server invite <email>")?;
            reject_extra(&mut args, "forma-server invite <email>")?;

            let pool = persistence::connect(&config.database_url, &config.data_dir)
                .await
                .context("could not open the Forma database to create an invite")?;
            let accounts = persistence::SqliteAccountRepository::new(pool.clone());
            let invite = accounts
                .invite(&email)
                .await
                .with_context(|| format!("could not invite {email:?}"))?;
            persistence::checkpoint_and_close(&pool)
                .await
                .context("the invite was written, but the database could not close cleanly")?;

            println!("invite open for {}", invite.email);
            println!("created {}", invite.created_at);
            return Ok(());
        }
        Some("api-key") => {
            let usage = "forma-server api-key <name>";
            let Some(name) = args.next().filter(|value| !value.trim().is_empty()) else {
                anyhow::bail!("usage: {usage}");
            };
            reject_extra(&mut args, usage)?;

            let pool = persistence::connect(&config.database_url, &config.data_dir)
                .await
                .context("could not open the Forma database to mint an access token")?;
            let accounts = persistence::SqliteAccountRepository::new(pool.clone());
            let keys = persistence::SqliteApiKeyRepository::new(pool.clone());

            // The bootstrap account, because that is the one a machine-local
            // command can name without asking who is running it. A key for
            // somebody else's account is theirs to create, from the browser.
            let account = accounts
                .bootstrap()
                .await
                .context("could not read the account to mint a token for")?;

            let created = keys
                .create(account.id, None, name.trim())
                .await
                .with_context(|| format!("could not mint an access token named {name:?}"))?;

            persistence::checkpoint_and_close(&pool)
                .await
                .context("the token was written, but the database could not close cleanly")?;

            // Once, and to stdout alone, so a shell can capture it without it
            // reaching a log. Forma keeps only the hash.
            println!("{}", created.token);
            eprintln!(
                "minted {:?} for account {}. This is the only time the value is shown.",
                created.key.name, created.key.account_id
            );
            return Ok(());
        }
        Some("grant-owner") => {
            let usage = "forma-server grant-owner <email> [subject-uuid]";
            let email = required_email(args.next(), usage)?;
            let subject = match args.next() {
                Some(raw) => AuthUserId(uuid::Uuid::parse_str(&raw).with_context(|| {
                    format!("subject UUID {raw:?} is malformed; usage: {usage}")
                })?),
                None => AuthUserId::new(),
            };
            reject_extra(&mut args, usage)?;

            let pool = persistence::connect(&config.database_url, &config.data_dir)
                .await
                .context("could not open the Forma database to grant ownership")?;
            let accounts = persistence::SqliteAccountRepository::new(pool.clone());
            let account = accounts
                .bootstrap()
                .await
                .context("could not find the bootstrap account to recover")?;
            let membership = accounts
                .grant(account.id, subject, &email, AccountRole::Owner)
                .await
                .with_context(|| {
                    format!("could not grant bootstrap-account ownership to {email:?}")
                })?;
            persistence::checkpoint_and_close(&pool)
                .await
                .context("the owner grant was written, but the database could not close cleanly")?;

            println!("owner granted for {}", membership.email);
            println!("account {}", membership.account_id);
            println!("subject {}", membership.user_id);
            return Ok(());
        }
        Some("invites") => {
            reject_extra(&mut args, "forma-server invites")?;

            let pool = persistence::connect(&config.database_url, &config.data_dir)
                .await
                .context("could not open the Forma database to list invites")?;
            let accounts = persistence::SqliteAccountRepository::new(pool.clone());
            let invites = accounts
                .list_invites()
                .await
                .context("could not list account invites")?;
            persistence::checkpoint_and_close(&pool)
                .await
                .context("the database could not close cleanly after listing invites")?;

            println!("STATUS\tEMAIL\tCREATED\tCLAIMED\tSUBJECT");
            for invite in invites {
                let status = if invite.is_open() { "open" } else { "claimed" };
                let claimed = invite.claimed_at.as_deref().unwrap_or("-");
                let subject = invite
                    .claimed_by_user_id
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| "-".to_string());
                println!(
                    "{status}\t{}\t{}\t{claimed}\t{subject}",
                    invite.email, invite.created_at
                );
            }
            return Ok(());
        }
        Some("--help" | "-h" | "help") => {
            println!("forma-server                                      run the server");
            println!(
                "forma-server backup [DIR]                         write a backup, default DATA_DIR/backups/<utc>"
            );
            println!(
                "forma-server invite <email>                       mint an invite so this address can create an account"
            );
            println!(
                "forma-server grant-owner <email> [subject-uuid]   attach an owner membership to the bootstrap account"
            );
            println!(
                "forma-server invites                              list invites, showing which are claimed"
            );
            println!(
                "forma-server api-key <name>                       mint a personal access token, printed once"
            );
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
    let mut state = state.with_account(account.id).with_mode(config.mode());

    // Hosted mode verifies the provider's tokens, and `binding_is_safe` has
    // already refused to start without the configuration this needs.
    if config.mode() == forma_server::config::AuthMode::Supabase {
        let (Some(issuer), Some(jwks)) = (&config.supabase_issuer, &config.supabase_jwks_url)
        else {
            anyhow::bail!("hosted mode needs FORMA_SUPABASE_ISSUER and FORMA_SUPABASE_JWKS_URL");
        };

        let mut verifier = forma_server::jwt::Verifier::new(
            issuer.clone(),
            config.supabase_audience.clone(),
            Box::new(forma_server::jwt::HttpJwks::new(jwks.clone())),
        );
        if let Some(resource) = &config.mcp_resource_url {
            verifier = verifier.for_resource(resource.trim_end_matches('/'));
        } else {
            // Without it no agent token can be accepted at all, which is right:
            // an unconfigured resource means there is no audience to demand, and
            // accepting anything in its place would make the check decoration.
            tracing::warn!(
                "FORMA_MCP_RESOURCE_URL is not set, so agent tokens cannot be accepted at /mcp"
            );
        }
        let verifier = std::sync::Arc::new(verifier);

        // Fetched once here rather than on somebody's first request, so a JWKS
        // URL that is wrong is a startup failure instead of an outage that only
        // appears when the first person tries to sign in.
        verifier
            .warm()
            .await
            .map_err(|e| anyhow::anyhow!("cannot read the provider's keys: {e}"))?;

        tracing::info!(issuer = %issuer, "verifying identity against the provider");
        state = state.with_verifier(verifier);
    }
    let state = state;

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

    let content_security_policy =
        forma_server::security::content_security_policy(config.supabase_url.as_deref())
            .map_err(anyhow::Error::msg)?;
    let token = config.remote_token.as_deref().map(auth::RemoteToken::new);

    let server = HttpServer::new(move || {
        let mut app = App::new()
            // Auth wraps outside tracing so an unauthorized call is still one
            // logged request, with its own status rather than silence.
            .wrap(actix_web::middleware::from_fn(auth::require_bearer))
            .wrap(DefaultHeaders::new().add((
                header::CONTENT_SECURITY_POLICY,
                content_security_policy.clone(),
            )))
            .wrap(actix_web::middleware::from_fn(telemetry::trace_request))
            .app_data(web::Data::new(state.clone()));

        if let Some(token) = token.clone() {
            app = app.app_data(web::Data::new(token));
        }

        let app = app.configure(routes::configure);

        // The built React app is served from the same origin, so the whole
        // product is one URL (plan section 58).
        if web_dir.is_dir() {
            let index = web_dir.join("index.html");
            app.service(
                actix_files::Files::new("/", &web_dir)
                    .index_file("index.html")
                    .prefer_utf8(true)
                    // Client-side routes are real entry points even though
                    // they are not files. API misses are consumed by their
                    // scope above and can never reach this HTML fallback.
                    .default_handler(fn_service(
                        move |request: actix_web::dev::ServiceRequest| {
                            let index = index.clone();
                            async move {
                                let (request, _) = request.into_parts();
                                let file = actix_files::NamedFile::open_async(index).await?;
                                let response = file.into_response(&request);
                                Ok(actix_web::dev::ServiceResponse::new(request, response))
                            }
                        },
                    )),
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

fn required_email(value: Option<String>, usage: &str) -> anyhow::Result<String> {
    let email = value.ok_or_else(|| anyhow::anyhow!("missing email; usage: {usage}"))?;
    if email.trim().is_empty() {
        anyhow::bail!("email is empty; usage: {usage}");
    }
    Ok(email)
}

fn reject_extra(args: &mut impl Iterator<Item = String>, usage: &str) -> anyhow::Result<()> {
    if let Some(extra) = args.next() {
        anyhow::bail!("unexpected argument {extra:?}; usage: {usage}");
    }
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
