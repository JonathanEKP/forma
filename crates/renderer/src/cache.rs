//! Render cache (plan section 72).
//!
//! The key is `design_id / version / page_id / scale / brand_version`. A
//! version never changes once written, which is what makes caching safe here:
//! an entry can never go stale, only unused.
//!
//! The brand version is part of it because a `$brand.*` token resolves at
//! render time, so editing a Brand Kit changes what a design looks like without
//! touching its version. Leaving it out of the key made the cache serve the old
//! picture after a brand edit — which only became reachable once the editor
//! could edit a brand at all.
//!
//! Unused is the problem. Every edit in the editor is a new version, and a
//! preview of a four-page carousel is about 100 KB, so an afternoon of dragging
//! would leave hundreds of megabytes of renders nobody will ask for again — an
//! older version's entry is never reused, because undo restores a snapshot as a
//! *new* version rather than returning to the old one.
//!
//! So the cache prunes itself: writing an entry for version N drops the
//! versions below `N - KEEP_RECENT_VERSIONS` for that design. The thing that
//! makes the cache grow is the thing that trims it, which means there is no
//! call site anywhere that can forget to. `sweep` handles the other direction —
//! designs that no longer exist at all.
//!
//! Assets are a different question and stay untouched: plan section 74 keeps
//! their bytes because historical versions still reference them.

use std::path::{Path, PathBuf};

use std::collections::HashSet;

use design_core::{DesignId, PageId, Version};

use crate::error::RenderError;

/// How many superseded versions of a design keep their renders.
///
/// Zero would be defensible — nothing reads them — but a render in flight when
/// the next version lands would lose its entry mid-request. Two is cheap
/// insurance that still bounds a design to three versions of cache.
pub const KEEP_RECENT_VERSIONS: u64 = 2;

/// What a cleanup removed, for the log.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Pruned {
    pub files: usize,
    pub bytes: u64,
}

impl Pruned {
    fn add(&mut self, other: Pruned) {
        self.files += other.files;
        self.bytes += other.bytes;
    }
}

pub struct RenderCache {
    root: PathBuf,
}

/// Scale is part of the key: a 2x export and a 1x preview are different bytes.
/// Encoded as an integer permille so the path never contains a float.
fn scale_key(scale: f32) -> u32 {
    (scale * 1000.0).round() as u32
}

impl RenderCache {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// `brand_version` is 0 for a project with no Brand Kit.
    pub fn path_for(
        &self,
        design: DesignId,
        version: Version,
        page: PageId,
        scale: f32,
        brand_version: u64,
    ) -> PathBuf {
        self.root
            .join(design.to_string())
            .join(version.to_string())
            .join(format!("{page}@{}b{brand_version}.png", scale_key(scale)))
    }

    pub fn get(
        &self,
        design: DesignId,
        version: Version,
        page: PageId,
        scale: f32,
        brand_version: u64,
    ) -> Option<Vec<u8>> {
        std::fs::read(self.path_for(design, version, page, scale, brand_version)).ok()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn put(
        &self,
        design: DesignId,
        version: Version,
        page: PageId,
        scale: f32,
        brand_version: u64,
        png: &[u8],
    ) -> Result<PathBuf, RenderError> {
        let path = self.path_for(design, version, page, scale, brand_version);
        let parent = path.parent().unwrap_or(Path::new("."));
        std::fs::create_dir_all(parent)
            .map_err(|e| RenderError::CacheUnwritable(format!("{}: {e}", parent.display())))?;
        std::fs::write(&path, png)
            .map_err(|e| RenderError::CacheUnwritable(format!("{}: {e}", path.display())))?;

        // Trimming here is what keeps the cache bounded without a background
        // task or a call site that could be forgotten. A failure to prune is a
        // fuller disk, not a failed render, so it is logged and swallowed.
        match self.retain_recent(design, version) {
            Ok(pruned) if pruned.files > 0 => tracing::debug!(
                design_id = %design,
                files = pruned.files,
                bytes = pruned.bytes,
                "pruned superseded renders"
            ),
            Err(error) => {
                tracing::warn!(%error, design_id = %design, "could not prune the render cache")
            }
            _ => {}
        }

        Ok(path)
    }

    /// Drops the renders of versions older than the current one, keeping
    /// `KEEP_RECENT_VERSIONS` behind it.
    pub fn retain_recent(&self, design: DesignId, current: Version) -> Result<Pruned, RenderError> {
        let oldest_kept = current.0.saturating_sub(KEEP_RECENT_VERSIONS);
        let design_dir = self.root.join(design.to_string());

        let mut pruned = Pruned::default();
        for entry in read_dir(&design_dir)? {
            // A directory whose name is not a version was not written by us.
            let Some(version) = entry
                .file_name()
                .to_str()
                .and_then(|name| name.parse::<u64>().ok())
            else {
                continue;
            };

            if version < oldest_kept {
                pruned.add(remove_dir(&entry.path())?);
            }
        }
        Ok(pruned)
    }

    /// Drops every render of designs that no longer exist.
    ///
    /// Deleting a project cascades in SQL and takes its designs with it, but
    /// nothing walks the filesystem to match — and a hook on every delete path
    /// is a hook that a future path forgets. Sweeping at startup against the
    /// designs that actually exist catches all of them, including anything left
    /// behind by a crash.
    pub fn sweep(&self, live: &HashSet<DesignId>) -> Result<Pruned, RenderError> {
        let mut pruned = Pruned::default();

        for entry in read_dir(&self.root)? {
            let Some(id) = entry
                .file_name()
                .to_str()
                .and_then(|name| name.parse::<uuid::Uuid>().ok())
                .map(DesignId)
            else {
                continue;
            };

            if !live.contains(&id) {
                pruned.add(remove_dir(&entry.path())?);
            }
        }
        Ok(pruned)
    }

    /// Drops every cached render of one design, used when a design is deleted.
    pub fn forget_design(&self, design: DesignId) -> Result<Pruned, RenderError> {
        remove_dir(&self.root.join(design.to_string()))
    }
}

/// Lists a directory, treating "it is not there" as "it is empty".
fn read_dir(dir: &Path) -> Result<Vec<std::fs::DirEntry>, RenderError> {
    match std::fs::read_dir(dir) {
        Ok(entries) => Ok(entries.flatten().collect()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(RenderError::CacheUnwritable(format!(
            "{}: {e}",
            dir.display()
        ))),
    }
}

/// Removes a directory, reporting what it held.
fn remove_dir(dir: &Path) -> Result<Pruned, RenderError> {
    let mut pruned = Pruned::default();
    for entry in read_dir(dir)? {
        let path = entry.path();
        if path.is_dir() {
            pruned.add(remove_dir(&path)?);
        } else {
            if let Ok(meta) = entry.metadata() {
                pruned.bytes += meta.len();
            }
            pruned.files += 1;
        }
    }

    match std::fs::remove_dir_all(dir) {
        Ok(()) => Ok(pruned),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Pruned::default()),
        Err(e) => Err(RenderError::CacheUnwritable(format!(
            "{}: {e}",
            dir.display()
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> (DesignId, Version, PageId) {
        (DesignId::new(), Version::INITIAL, PageId::new())
    }

    #[test]
    fn round_trips_a_render() {
        let dir = tempfile::tempdir().unwrap();
        let cache = RenderCache::new(dir.path());
        let (design, version, page) = key();

        assert!(cache.get(design, version, page, 1.0, 1).is_none());
        cache
            .put(design, version, page, 1.0, 1, b"fake png")
            .unwrap();
        assert_eq!(
            cache.get(design, version, page, 1.0, 1).unwrap(),
            b"fake png"
        );
    }

    #[test]
    fn a_new_version_is_a_different_entry() {
        let dir = tempfile::tempdir().unwrap();
        let cache = RenderCache::new(dir.path());
        let (design, version, page) = key();

        cache.put(design, version, page, 1.0, 1, b"v1").unwrap();
        // Nothing ever overwrites an existing version, so the old entry stands.
        assert!(cache.get(design, version.next(), page, 1.0, 1).is_none());
        assert_eq!(cache.get(design, version, page, 1.0, 1).unwrap(), b"v1");
    }

    /// Regression: a brand edit changes what a design looks like without
    /// changing its version, so the brand has to be in the key. It was not, and
    /// the preview kept serving the picture from before the edit.
    #[test]
    fn the_brand_version_is_part_of_the_key() {
        let dir = tempfile::tempdir().unwrap();
        let cache = RenderCache::new(dir.path());
        let (design, version, page) = key();

        cache.put(design, version, page, 1.0, 1, b"dark").unwrap();

        // Same design, same version, different brand: not a hit.
        assert!(cache.get(design, version, page, 1.0, 2).is_none());
        assert_eq!(cache.get(design, version, page, 1.0, 1).unwrap(), b"dark");

        cache.put(design, version, page, 1.0, 2, b"green").unwrap();
        assert_eq!(cache.get(design, version, page, 1.0, 2).unwrap(), b"green");
    }

    /// A project with no Brand Kit still caches, under version 0.
    #[test]
    fn a_project_without_a_brand_still_caches() {
        let dir = tempfile::tempdir().unwrap();
        let cache = RenderCache::new(dir.path());
        let (design, version, page) = key();

        cache
            .put(design, version, page, 1.0, 0, b"literals only")
            .unwrap();
        assert_eq!(
            cache.get(design, version, page, 1.0, 0).unwrap(),
            b"literals only"
        );
    }

    #[test]
    fn scale_is_part_of_the_key() {
        let dir = tempfile::tempdir().unwrap();
        let cache = RenderCache::new(dir.path());
        let (design, version, page) = key();

        cache
            .put(design, version, page, 1.0, 1, b"preview")
            .unwrap();
        cache.put(design, version, page, 2.0, 1, b"export").unwrap();

        assert_eq!(
            cache.get(design, version, page, 1.0, 1).unwrap(),
            b"preview"
        );
        assert_eq!(cache.get(design, version, page, 2.0, 1).unwrap(), b"export");
    }

    /// The growth this exists to stop: an afternoon of editing is hundreds of
    /// versions, and only the recent ones are worth keeping.
    #[test]
    fn writing_a_version_drops_the_ones_it_superseded() {
        let dir = tempfile::tempdir().unwrap();
        let cache = RenderCache::new(dir.path());
        let (design, _, page) = key();

        for version in 1..=20u64 {
            cache
                .put(design, Version(version), page, 1.0, 1, b"render")
                .unwrap();
        }

        // The current version plus KEEP_RECENT_VERSIONS behind it.
        for version in 18..=20u64 {
            assert!(
                cache.get(design, Version(version), page, 1.0, 1).is_some(),
                "version {version} should still be cached"
            );
        }
        for version in 1..=17u64 {
            assert!(
                cache.get(design, Version(version), page, 1.0, 1).is_none(),
                "version {version} should have been pruned"
            );
        }

        let kept = std::fs::read_dir(dir.path().join(design.to_string()))
            .unwrap()
            .count();
        assert_eq!(kept, 3, "the cache should hold three versions, not {kept}");
    }

    #[test]
    fn pruning_reports_what_it_freed() {
        let dir = tempfile::tempdir().unwrap();
        let cache = RenderCache::new(dir.path());
        let (design, _, page) = key();

        for version in 1..=5u64 {
            cache
                .put(design, Version(version), page, 1.0, 1, b"0123456789")
                .unwrap();
        }

        // Versions 1 and 2 are already gone; pruning at 5 removes nothing more.
        let pruned = cache.retain_recent(design, Version(5)).unwrap();
        assert_eq!(pruned, Pruned::default());

        // Pretending the design jumped ahead removes what is now old.
        let pruned = cache.retain_recent(design, Version(9)).unwrap();
        assert_eq!(pruned.files, 3);
        assert_eq!(pruned.bytes, 30);
    }

    /// Deleting a project cascades in SQL; nothing walks the filesystem, so the
    /// renders of a design that no longer exists would otherwise stay forever.
    #[test]
    fn sweeping_drops_designs_that_no_longer_exist() {
        let dir = tempfile::tempdir().unwrap();
        let cache = RenderCache::new(dir.path());

        let live = DesignId::new();
        let deleted = DesignId::new();
        let page = PageId::new();
        cache.put(live, Version(1), page, 1.0, 1, b"keep").unwrap();
        cache
            .put(deleted, Version(1), page, 1.0, 1, b"drop")
            .unwrap();

        let pruned = cache.sweep(&HashSet::from([live])).unwrap();

        assert_eq!(pruned.files, 1);
        assert!(cache.get(live, Version(1), page, 1.0, 1).is_some());
        assert!(cache.get(deleted, Version(1), page, 1.0, 1).is_none());
    }

    /// Cleanup must never reach outside what it wrote.
    #[test]
    fn sweeping_ignores_anything_it_did_not_write() {
        let dir = tempfile::tempdir().unwrap();
        let cache = RenderCache::new(dir.path());

        let stray = dir.path().join("not-a-design-id");
        std::fs::create_dir_all(&stray).unwrap();
        std::fs::write(stray.join("important.txt"), b"leave me").unwrap();

        cache.sweep(&HashSet::new()).unwrap();
        assert!(
            stray.join("important.txt").exists(),
            "swept a stranger's file"
        );
    }

    #[test]
    fn sweeping_an_empty_cache_is_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let cache = RenderCache::new(dir.path().join("never-written"));
        assert_eq!(cache.sweep(&HashSet::new()).unwrap(), Pruned::default());
    }

    #[test]
    fn forgetting_a_design_removes_every_version() {
        let dir = tempfile::tempdir().unwrap();
        let cache = RenderCache::new(dir.path());
        let (design, version, page) = key();

        cache.put(design, version, page, 1.0, 1, b"a").unwrap();
        cache
            .put(design, version.next(), page, 1.0, 1, b"b")
            .unwrap();
        cache.forget_design(design).unwrap();

        assert!(cache.get(design, version, page, 1.0, 1).is_none());
        assert!(cache.get(design, version.next(), page, 1.0, 1).is_none());
    }
}
