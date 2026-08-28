//! Render cache (plan section 72).
//!
//! The key is `design_id / version / page_id / scale`. A version never changes
//! once written, which is precisely what makes caching safe here: an entry can
//! never go stale, only unused. Cleanup is deferred (plan section 74).

use std::path::{Path, PathBuf};

use design_core::{DesignId, PageId, Version};

use crate::error::RenderError;

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

    pub fn path_for(
        &self,
        design: DesignId,
        version: Version,
        page: PageId,
        scale: f32,
    ) -> PathBuf {
        self.root
            .join(design.to_string())
            .join(version.to_string())
            .join(format!("{page}@{}.png", scale_key(scale)))
    }

    pub fn get(
        &self,
        design: DesignId,
        version: Version,
        page: PageId,
        scale: f32,
    ) -> Option<Vec<u8>> {
        std::fs::read(self.path_for(design, version, page, scale)).ok()
    }

    pub fn put(
        &self,
        design: DesignId,
        version: Version,
        page: PageId,
        scale: f32,
        png: &[u8],
    ) -> Result<PathBuf, RenderError> {
        let path = self.path_for(design, version, page, scale);
        let parent = path.parent().unwrap_or(Path::new("."));
        std::fs::create_dir_all(parent)
            .map_err(|e| RenderError::CacheUnwritable(format!("{}: {e}", parent.display())))?;
        std::fs::write(&path, png)
            .map_err(|e| RenderError::CacheUnwritable(format!("{}: {e}", path.display())))?;
        Ok(path)
    }

    /// Drops every cached render of one design, used when a design is deleted.
    pub fn forget_design(&self, design: DesignId) -> Result<(), RenderError> {
        let dir = self.root.join(design.to_string());
        if dir.exists() {
            std::fs::remove_dir_all(&dir)
                .map_err(|e| RenderError::CacheUnwritable(format!("{}: {e}", dir.display())))?;
        }
        Ok(())
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

        assert!(cache.get(design, version, page, 1.0).is_none());
        cache.put(design, version, page, 1.0, b"fake png").unwrap();
        assert_eq!(cache.get(design, version, page, 1.0).unwrap(), b"fake png");
    }

    #[test]
    fn a_new_version_is_a_different_entry() {
        let dir = tempfile::tempdir().unwrap();
        let cache = RenderCache::new(dir.path());
        let (design, version, page) = key();

        cache.put(design, version, page, 1.0, b"v1").unwrap();
        // Nothing ever overwrites an existing version, so the old entry stands.
        assert!(cache.get(design, version.next(), page, 1.0).is_none());
        assert_eq!(cache.get(design, version, page, 1.0).unwrap(), b"v1");
    }

    #[test]
    fn scale_is_part_of_the_key() {
        let dir = tempfile::tempdir().unwrap();
        let cache = RenderCache::new(dir.path());
        let (design, version, page) = key();

        cache.put(design, version, page, 1.0, b"preview").unwrap();
        cache.put(design, version, page, 2.0, b"export").unwrap();

        assert_eq!(cache.get(design, version, page, 1.0).unwrap(), b"preview");
        assert_eq!(cache.get(design, version, page, 2.0).unwrap(), b"export");
    }

    #[test]
    fn forgetting_a_design_removes_every_version() {
        let dir = tempfile::tempdir().unwrap();
        let cache = RenderCache::new(dir.path());
        let (design, version, page) = key();

        cache.put(design, version, page, 1.0, b"a").unwrap();
        cache.put(design, version.next(), page, 1.0, b"b").unwrap();
        cache.forget_design(design).unwrap();

        assert!(cache.get(design, version, page, 1.0).is_none());
        assert!(cache.get(design, version.next(), page, 1.0).is_none());
    }
}
