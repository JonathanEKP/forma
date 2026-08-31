//! Export (plan section 71).
//!
//! An export is a render pinned to a version. The bytes come off the same
//! pipeline as the editor preview, so what a user downloads is what they were
//! looking at; and because a version's document never changes, an export stays
//! reproducible after the design moves on.

use design_core::{AccountId, BrandKit, DesignId, DesignRecord, Export, ExportId, ExportScope, PageId};
use persistence::{export_storage_key, AssetRepository, ExportRepository, ObjectStore};
use renderer::{FontRegistry, RenderCache};

use crate::error::{ApplicationError, Result};
use crate::renders;

/// One rendered file inside an export.
#[derive(Debug, Clone, PartialEq)]
pub struct ExportedFile {
    pub filename: String,
    pub storage_key: String,
    pub byte_size: i64,
}

/// Plan section 71: a single page is named after the design, a carousel is a
/// numbered sequence. Pages are one-based, because the name is for a human.
fn filename(design_name: &str, scope: ExportScope, index: usize) -> String {
    match scope {
        ExportScope::SinglePage => format!("{}-page-{:02}.png", slug(design_name), index + 1),
        ExportScope::AllPages => format!("{:02}.png", index + 1),
    }
}

/// Keeps a design name usable as a filename on any of the platforms the plan
/// targets, without depending on what a user typed.
fn slug(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();

    let collapsed = cleaned
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");

    if collapsed.is_empty() {
        "design".to_string()
    } else {
        collapsed
    }
}

/// Renders and stores an export, then records it.
///
/// The whole set is rendered before anything is recorded: a row pointing at
/// files that are only half there would be worse than no row at all.
#[allow(clippy::too_many_arguments)]
pub async fn create(
    exports: &dyn ExportRepository,
    assets: &dyn AssetRepository,
    store: &dyn ObjectStore,
    cache: &RenderCache,
    fonts: &FontRegistry,
    account: AccountId,
    record: &DesignRecord,
    brand: Option<&BrandKit>,
    scope: ExportScope,
    page_id: Option<PageId>,
    scale: f32,
) -> Result<(Export, Vec<ExportedFile>)> {
    let document = &record.document;

    let pages: Vec<(usize, PageId)> = match scope {
        ExportScope::SinglePage => {
            let wanted = match page_id {
                Some(id) => id,
                None => document.pages.first().map(|p| p.id).ok_or_else(|| {
                    ApplicationError::rejected(
                        "INVALID_OP",
                        "this design has no pages to export",
                        "Add a page before exporting.",
                    )
                })?,
            };
            let index = document
                .pages
                .iter()
                .position(|p| p.id == wanted)
                .ok_or(design_core::DomainError::PageNotFound(wanted))?;
            vec![(index, wanted)]
        }
        ExportScope::AllPages => document
            .pages
            .iter()
            .enumerate()
            .map(|(i, p)| (i, p.id))
            .collect(),
    };

    if pages.is_empty() {
        return Err(ApplicationError::rejected(
            "INVALID_OP",
            "this design has no pages to export",
            "Add a page before exporting.",
        ));
    }

    let export_id = ExportId::new();
    let first_page = pages[0].1;
    let mut files = Vec::new();

    for (index, page) in pages {
        let loaded = renders::load_page_assets(assets, store, account, document, page).await?;
        let png = renders::render_png(
            cache,
            record.id,
            record.version,
            document,
            page,
            brand,
            fonts,
            &loaded,
            scale,
        )?;

        let name = filename(&record.name, scope, index);
        let key = export_storage_key(&export_id.to_string(), &name);
        store.put(&key, &png).await?;

        files.push(ExportedFile {
            filename: name,
            storage_key: key,
            byte_size: png.len() as i64,
        });
    }

    let total: i64 = files.iter().map(|f| f.byte_size).sum();

    // For a single page the row points straight at the file. For a set it
    // points at the directory holding them, which is what a future ZIP would
    // replace with one archive (plan section 71).
    let storage_key = match scope {
        ExportScope::SinglePage => files[0].storage_key.clone(),
        ExportScope::AllPages => format!("exports/{export_id}/"),
    };

    let export = Export {
        id: export_id,
        design_id: record.id,
        design_version: record.version,
        scope,
        page_id: match scope {
            ExportScope::SinglePage => Some(first_page),
            ExportScope::AllPages => None,
        },
        scale,
        storage_key,
        byte_size: total,
        created_at: String::new(),
    };

    Ok((exports.create(account, &export).await?, files))
}

pub async fn get(
    exports: &dyn ExportRepository,
    account: AccountId,
    id: ExportId,
) -> Result<Export> {
    Ok(exports.get(account, id).await?)
}

pub async fn list(
    exports: &dyn ExportRepository,
    account: AccountId,
    design_id: DesignId,
) -> Result<Vec<Export>> {
    Ok(exports.list_for_design(account, design_id).await?)
}

/// The filenames an export produced, derived from the version it pinned.
///
/// They are not stored: the naming of section 71 is a function of the scope,
/// the design name and the page count, and the version those were taken from
/// can never change.
pub fn filenames(export: &Export, design_name: &str, page_count: usize) -> Vec<String> {
    match export.scope {
        ExportScope::SinglePage => export
            .storage_key
            .rsplit('/')
            .next()
            .map(|n| vec![n.to_string()])
            .unwrap_or_default(),
        ExportScope::AllPages => (0..page_count)
            .map(|i| filename(design_name, ExportScope::AllPages, i))
            .collect(),
    }
}

/// Reads one file of an export back.
/// Takes no `AccountId` on purpose: the `Export` it is handed was fetched
/// through a scoped read, so the boundary was crossed before this is reached.
/// Asking for the account again would suggest this function enforces something,
/// and a check that enforces nothing is worse than no check.
pub async fn download(
    store: &dyn ObjectStore,
    export: &Export,
    filename: Option<&str>,
) -> Result<Vec<u8>> {
    let key = match (export.scope, filename) {
        (ExportScope::SinglePage, _) => export.storage_key.clone(),
        (ExportScope::AllPages, Some(name)) => {
            // The name comes from a URL, so it must not be able to walk out of
            // the export's own directory.
            if name.contains('/') || name.contains('\\') || name.contains("..") {
                return Err(ApplicationError::rejected(
                    "INVALID_OP",
                    format!("{name:?} is not a file of this export"),
                    "Use one of the names that GET /exports/:id listed.",
                ));
            }
            format!("{}{name}", export.storage_key)
        }
        (ExportScope::AllPages, None) => {
            return Err(ApplicationError::rejected(
                "INVALID_OP",
                "an all-pages export has no single file to download",
                "Name a file with ?file=, using one of the names GET /exports/:id listed. \
                 A ZIP of the whole set is not implemented in V1.",
            ))
        }
    };

    Ok(store.get(&key).await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_single_page_is_named_after_its_design() {
        assert_eq!(
            filename("Launch Teaser", ExportScope::SinglePage, 0),
            "launch-teaser-page-01.png"
        );
    }

    #[test]
    fn a_carousel_is_a_numbered_sequence() {
        assert_eq!(filename("anything", ExportScope::AllPages, 0), "01.png");
        assert_eq!(filename("anything", ExportScope::AllPages, 9), "10.png");
    }

    #[test]
    fn a_name_made_only_of_punctuation_still_produces_a_filename() {
        assert_eq!(slug("///"), "design");
        assert_eq!(slug(""), "design");
    }

    #[test]
    fn a_name_cannot_smuggle_a_path_into_the_filename() {
        assert_eq!(slug("../../etc/passwd"), "etc-passwd");
        assert!(!filename("../escape", ExportScope::SinglePage, 0).contains('/'));
    }
}
