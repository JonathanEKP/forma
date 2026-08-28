//! Plan section 34: the renderer must load the same fonts everywhere, so it
//! reads a directory the repo controls and never the host's installed fonts.

use std::path::Path;
use std::sync::Arc;

use usvg::fontdb;

use crate::error::RenderError;

pub struct FontRegistry {
    db: Arc<fontdb::Database>,
    families: Vec<String>,
}

impl FontRegistry {
    /// Loads every font in `dir`. The database starts empty, so a family that is
    /// installed on the host but absent here will not silently resolve.
    pub fn from_dir(dir: impl AsRef<Path>) -> Result<Self, RenderError> {
        let dir = dir.as_ref();
        if !dir.is_dir() {
            return Err(RenderError::FontDirUnreadable(dir.display().to_string()));
        }

        let mut db = fontdb::Database::new();
        db.load_fonts_dir(dir);

        if db.is_empty() {
            return Err(RenderError::FontNotFound(format!(
                "no usable font files in {}",
                dir.display()
            )));
        }

        let mut families: Vec<String> = db
            .faces()
            .filter_map(|face| face.families.first().map(|(name, _)| name.clone()))
            .collect();
        families.sort();
        families.dedup();

        Ok(Self {
            db: Arc::new(db),
            families,
        })
    }

    /// The catalog `get_brand`/`get_font_catalog` will expose (plan section 13):
    /// the agent must not invent families the renderer cannot draw.
    pub fn families(&self) -> &[String] {
        &self.families
    }

    pub fn contains(&self, family: &str) -> bool {
        self.families.iter().any(|f| f == family)
    }

    pub fn db(&self) -> Arc<fontdb::Database> {
        Arc::clone(&self.db)
    }
}
