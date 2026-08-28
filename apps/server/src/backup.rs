//! Taking a backup (plan section 77, Phase 10).
//!
//! Two things here are irreplaceable: the database, which holds every project,
//! design and version snapshot, and the uploaded asset bytes. Everything else
//! under the data directory is derived and deliberately left out —
//! `previews/` is a cache that regenerates on demand, and `exports/` can be
//! produced again from the version each export pinned, which is in the
//! database. Copying them would multiply the size of a backup to protect
//! nothing.
//!
//! The database is copied with `VACUUM INTO` rather than by copying the file.
//! A running server keeps recent writes in the write-ahead log, so a plain copy
//! of `forma.db` can miss them, and copying the three files separately is not
//! atomic. `VACUUM INTO` asks SQLite for a consistent snapshot in one file, and
//! works while the server is running.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context};
use serde::Serialize;
use sqlx::SqlitePool;

use crate::config::Config;

#[derive(Debug, Serialize)]
pub struct Summary {
    pub out: PathBuf,
    pub database_bytes: u64,
    pub asset_files: usize,
    pub asset_bytes: u64,
}

#[derive(Debug, Serialize)]
struct Manifest {
    forma_version: &'static str,
    document_schema_version: u32,
    created_at: String,
    projects: i64,
    designs: i64,
    design_versions: i64,
    assets: i64,
    asset_files: usize,
    /// Said out loud, so restoring does not look like it lost something.
    excluded: [&'static str; 2],
}

/// Writes a backup into `out`, which must not already hold one.
pub async fn run(config: &Config, out: &Path) -> anyhow::Result<Summary> {
    // Never write into a directory that already has something in it: a backup
    // that silently merged with an older one would restore as a mixture.
    if out.exists()
        && out
            .read_dir()
            .with_context(|| format!("reading {}", out.display()))?
            .next()
            .is_some()
    {
        bail!(
            "{} already exists and is not empty; back up into a new directory",
            out.display()
        );
    }
    std::fs::create_dir_all(out).with_context(|| format!("creating {}", out.display()))?;

    // The same connect the server uses, so a backup applies any pending
    // migration rather than snapshotting a schema the code cannot read back.
    let pool = persistence::connect(&config.database_url, &config.data_dir)
        .await
        .context("opening the database")?;
    let database = out.join("forma.db");

    // VACUUM INTO refuses to overwrite, which is the guard we want anyway.
    sqlx::query("VACUUM INTO ?")
        .bind(database.to_string_lossy().as_ref())
        .execute(&pool)
        .await
        .context("copying the database")?;

    let counts = counts(&pool).await?;
    let (asset_files, asset_bytes) = copy_assets(&config.data_dir, out)?;

    let manifest = Manifest {
        forma_version: env!("CARGO_PKG_VERSION"),
        document_schema_version: design_core::SCHEMA_VERSION,
        created_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        projects: counts.0,
        designs: counts.1,
        design_versions: counts.2,
        assets: counts.3,
        asset_files,
        excluded: [
            "previews/ — a render cache, rebuilt on demand",
            "exports/ — regenerable from the version each export pinned",
        ],
    };
    std::fs::write(
        out.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )
    .context("writing the manifest")?;

    persistence::checkpoint_and_close(&pool).await?;

    Ok(Summary {
        out: out.to_path_buf(),
        database_bytes: std::fs::metadata(&database)?.len(),
        asset_files,
        asset_bytes,
    })
}

async fn counts(pool: &SqlitePool) -> anyhow::Result<(i64, i64, i64, i64)> {
    let one = |table: &'static str| async move {
        sqlx::query_scalar::<_, i64>(&format!("SELECT COUNT(*) FROM {table}"))
            .fetch_one(pool)
            .await
    };

    Ok((
        one("projects").await?,
        one("designs").await?,
        one("design_versions").await?,
        one("assets WHERE deleted_at IS NULL").await?,
    ))
}

/// Copies the uploaded bytes.
///
/// Assets are content-addressed, so a file never changes once written and a
/// copy taken while the server runs cannot tear. An upload that lands mid-copy
/// may be missed, which is the ordinary limitation of a hot backup: its row
/// would be in the database with no file, and re-uploading fixes it.
fn copy_assets(data_dir: &Path, out: &Path) -> anyhow::Result<(usize, u64)> {
    let source = data_dir.join("assets");
    let target = out.join("assets");

    if !source.is_dir() {
        return Ok((0, 0));
    }
    copy_tree(&source, &target)
}

fn copy_tree(source: &Path, target: &Path) -> anyhow::Result<(usize, u64)> {
    std::fs::create_dir_all(target).with_context(|| format!("creating {}", target.display()))?;

    let mut files = 0;
    let mut bytes = 0;

    for entry in std::fs::read_dir(source)
        .with_context(|| format!("reading {}", source.display()))?
        .flatten()
    {
        let from = entry.path();
        let to = target.join(entry.file_name());

        if from.is_dir() {
            let (f, b) = copy_tree(&from, &to)?;
            files += f;
            bytes += b;
        } else {
            bytes +=
                std::fs::copy(&from, &to).with_context(|| format!("copying {}", from.display()))?;
            files += 1;
        }
    }

    Ok((files, bytes))
}

/// Where a backup goes when the caller does not say.
pub fn default_destination(data_dir: &Path) -> PathBuf {
    data_dir
        .join("backups")
        .join(chrono::Utc::now().format("%Y-%m-%dT%H-%M-%SZ").to_string())
}
