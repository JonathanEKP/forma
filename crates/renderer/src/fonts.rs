//! Plan section 34: the renderer must load the same fonts everywhere, so it
//! reads a directory the repo controls and never the host's installed fonts.

use std::path::{Path, PathBuf};
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

        // `fontdb::load_fonts_dir` walks the directory in `read_dir` order, which
        // the filesystem decides. That order is the face order, and the face order
        // decides which face a missing glyph falls back to, so the same repo could
        // render Greek text in a different weight on another machine. Loading the
        // files sorted makes the database identical everywhere.
        let mut files = Vec::new();
        collect_font_files(dir, 0, &mut files);
        files.sort();

        let mut db = fontdb::Database::new();
        for file in &files {
            if let Err(error) = db.load_font_file(file) {
                tracing::warn!(path = %file.display(), %error, "skipping unreadable font file");
            }
        }

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

/// Directories nest no deeper than this. The limit is what stops a symlink loop
/// from recursing forever; `data/fonts` is flat, so it is never approached.
const MAX_FONT_DIR_DEPTH: usize = 8;

fn collect_font_files(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > MAX_FONT_DIR_DEPTH {
        tracing::warn!(path = %dir.display(), "font directory nested too deeply, not descending");
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for path in entries.flatten().map(|entry| entry.path()) {
        if path.is_dir() {
            collect_font_files(&path, depth + 1, out);
        } else if is_font_file(&path) {
            out.push(path);
        }
    }
}

fn is_font_file(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "ttf" | "ttc" | "otf" | "otc"
            )
        })
}

/// Chooses the face that draws a glyph the requested family lacks.
///
/// usvg's default selector returns the *first* face in the database that has the
/// glyph, and its style guard rejects a candidate only when style, weight and
/// stretch all differ — so a Bold face happily answers for Regular text. This one
/// scores every candidate against the face being replaced, so a weight-400 layer
/// falls back to a 400 face whenever the database holds one. Ties break on
/// database order, which `from_dir` has already made deterministic.
pub fn fallback_selector() -> usvg::FallbackSelectionFn<'static> {
    Box::new(|glyph, already_used, db| {
        let db: &fontdb::Database = db;
        let base = db.face(*already_used.first()?)?;
        let (style, weight, stretch) = (base.style, base.weight, base.stretch);

        db.faces()
            .filter(|face| !already_used.contains(&face.id))
            .filter(|face| has_glyph(db, face.id, glyph))
            .min_by_key(|face| {
                (
                    u16::from(face.style != style),
                    weight.0.abs_diff(face.weight.0),
                    u16::from(face.stretch != stretch),
                )
            })
            .map(|face| face.id)
    })
}

/// Whether a face can draw a glyph. usvg keeps its own `has_char` private, so
/// this mirrors it with the same parser it shapes with — a second implementation
/// could answer differently and we would fall back to a face that then draws tofu.
fn has_glyph(db: &fontdb::Database, id: fontdb::ID, glyph: char) -> bool {
    let found = db.with_face_data(id, |data, index| {
        skrifa::FontRef::from_index(data, index)
            .ok()
            .and_then(|font| skrifa::charmap::Charmap::new(&font).map(glyph))
            .is_some()
    });
    found == Some(true)
}
