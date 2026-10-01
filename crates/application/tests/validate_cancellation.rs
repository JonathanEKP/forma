//! `validate` stops when its caller abandons it (Pontis plan, section 11 step 5).
//!
//! Nothing here waits on a clock. The signal fires on a condition the test
//! controls — the object store being asked for a particular page's image — and
//! the evidence is what the store was and was not asked for afterwards.

use std::future::Future;
use std::path::PathBuf;
use std::sync::Mutex;
use std::task::{Context, Poll, Waker};

use application::Cancellation;
use async_trait::async_trait;
use design_core::{
    AccountId, Asset, AssetId, AssetKind, AssetOrigin, Color, DesignColor, DesignDocument,
    DesignFormat, DesignId, DesignRecord, FormatPreset, ImageFit, ImageLayer, Layer, LayerCommon,
    LayerId, Page, ProjectId, TextAlign, TextLayer, Transform, Version,
};
use persistence::{AssetRepository, ObjectStore, PersistenceError};
use renderer::{FontRegistry, RenderCache};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fonts() -> FontRegistry {
    FontRegistry::from_dir(root().join("data/fonts")).expect("data/fonts must load")
}

fn account() -> AccountId {
    AccountId(uuid::Uuid::parse_str(design_core::BOOTSTRAP_ACCOUNT).expect("a constant uuid"))
}

fn common(transform: Transform) -> LayerCommon {
    LayerCommon {
        id: LayerId::new(),
        name: "Layer".to_string(),
        role: None,
        visible: true,
        locked: false,
        opacity: 1.0,
        transform,
    }
}

fn headline(fonts: &FontRegistry) -> Layer {
    Layer::Text(TextLayer {
        common: common(Transform::new(80.0, 120.0, 900.0, 300.0)),
        text: "CANCEL".to_string(),
        font_family: fonts.families()[0].clone(),
        font_size: 90.0,
        font_weight: 700,
        line_height: 1.1,
        letter_spacing: 0.0,
        align: TextAlign::Left,
        color: DesignColor::Literal(Color::rgb(0xFF, 0xFF, 0xFF)),
    })
}

/// A design of `pages` pages, each holding its own photograph when `images` is
/// set, so which page validate reached shows up as which bytes it fetched.
fn design(fonts: &FontRegistry, pages: usize, images: bool) -> (DesignRecord, Vec<AssetId>) {
    let mut document = DesignDocument::new(DesignFormat::from_preset(FormatPreset::Square));
    let mut asset_ids = Vec::new();
    for n in 0..pages {
        let mut page = Page::new(format!("Page {}", n + 1));
        if images {
            let asset_id = AssetId::new();
            asset_ids.push(asset_id);
            page.layers.push(Layer::Image(ImageLayer {
                common: common(Transform::new(0.0, 0.0, 1080.0, 1080.0)),
                asset_id,
                fit: ImageFit::Cover,
                focal_point: Default::default(),
                overlay: None,
            }));
        }
        page.layers.push(headline(fonts));
        document.pages.push(page);
    }
    let record = DesignRecord {
        id: DesignId::new(),
        project_id: ProjectId::new(),
        name: "Under test".to_string(),
        version: Version::INITIAL,
        created_at: "2026-10-01T00:00:00.000Z".to_string(),
        updated_at: "2026-10-01T00:00:00.000Z".to_string(),
        document,
    };
    (record, asset_ids)
}

fn storage_key(id: AssetId) -> String {
    format!("assets/{}", id.0)
}

/// Every asset is the hero fixture, stored under a key that names its id.
struct Assets;

#[async_trait]
impl AssetRepository for Assets {
    async fn create(&self, _: AccountId, _: &Asset) -> persistence::Result<Asset> {
        unimplemented!("validate never creates an asset")
    }

    async fn get(&self, _: AccountId, id: AssetId) -> persistence::Result<Asset> {
        Ok(Asset {
            id,
            project_id: ProjectId::new(),
            hash: String::new(),
            kind: AssetKind::Raster,
            mime_type: "image/png".to_string(),
            width: 1,
            height: 1,
            byte_size: 0,
            storage_key: storage_key(id),
            original_filename: None,
            origin: AssetOrigin::UserUpload,
            attribution: None,
            created_at: String::new(),
        })
    }

    async fn list_for_project(
        &self,
        _: AccountId,
        _: ProjectId,
    ) -> persistence::Result<Vec<Asset>> {
        unimplemented!("validate never lists assets")
    }

    async fn find_by_hash(
        &self,
        _: AccountId,
        _: ProjectId,
        _: &str,
    ) -> persistence::Result<Option<Asset>> {
        unimplemented!("validate never deduplicates")
    }

    async fn soft_delete(&self, _: AccountId, _: AssetId) -> persistence::Result<()> {
        unimplemented!("validate never deletes")
    }
}

/// Records every fetch, and fires `cancel` when asked for `trip` — the moment
/// validate starts loading that page's image.
struct Store {
    bytes: Vec<u8>,
    fetched: Mutex<Vec<String>>,
    trip: Option<(String, Cancellation)>,
}

impl Store {
    fn new(trip: Option<(String, Cancellation)>) -> Self {
        Self {
            bytes: std::fs::read(root().join("fixtures/assets/hero.png")).expect("hero fixture"),
            fetched: Mutex::new(Vec::new()),
            trip,
        }
    }

    fn fetched(&self) -> Vec<String> {
        self.fetched.lock().unwrap().clone()
    }
}

#[async_trait]
impl ObjectStore for Store {
    async fn put(&self, _: &str, _: &[u8]) -> persistence::Result<()> {
        Err(PersistenceError::NotFound(
            "validate must write nothing".to_string(),
        ))
    }

    async fn get(&self, key: &str) -> persistence::Result<Vec<u8>> {
        self.fetched.lock().unwrap().push(key.to_string());
        if let Some((trip, cancel)) = &self.trip {
            if trip == key {
                cancel.cancel();
            }
        }
        Ok(self.bytes.clone())
    }

    async fn exists(&self, _: &str) -> persistence::Result<bool> {
        Ok(true)
    }

    async fn delete(&self, _: &str) -> persistence::Result<()> {
        Err(PersistenceError::NotFound(
            "validate must delete nothing".to_string(),
        ))
    }
}

/// The control for the test below: unsignalled, validate reaches every page.
#[tokio::test]
async fn an_uncancelled_validate_reaches_every_page() {
    let fonts = fonts();
    let (record, asset_ids) = design(&fonts, 3, true);
    let store = Store::new(None);

    application::renders::validate(
        account(),
        &record,
        None,
        &fonts,
        &Assets,
        &store,
        &Cancellation::new(),
    )
    .await
    .expect("an uncancelled validate finishes");

    let every_page: Vec<_> = asset_ids.iter().map(|id| storage_key(*id)).collect();
    assert_eq!(store.fetched(), every_page);
}

#[tokio::test]
async fn a_validate_cancelled_mid_flight_stops_instead_of_finishing() {
    let fonts = fonts();
    let (record, asset_ids) = design(&fonts, 3, true);
    let cancel = Cancellation::new();
    // Fires while page 2's image is being fetched: page 1 is fully measured,
    // page 2 is half way, page 3 not started.
    let store = Store::new(Some((storage_key(asset_ids[1]), cancel.clone())));

    let outcome =
        application::renders::validate(account(), &record, None, &fonts, &Assets, &store, &cancel)
            .await;

    match outcome {
        Err(error) => assert_eq!(error.code(), "CANCELLED"),
        Ok(report) => panic!("a cancelled validate returned a report: {report:?}"),
    }
    // Page 3 was never begun. The control above shows it would have been.
    assert_eq!(
        store.fetched(),
        vec![storage_key(asset_ids[0]), storage_key(asset_ids[1])],
    );
}

/// The render cache is keyed by version and brand, so an entry written by a run
/// that then stopped would be served later as a finished render. Validate holds
/// no cache today; this is what fails if someone hands it one.
#[tokio::test]
async fn a_cancelled_validate_leaves_no_render_cache_entry() {
    let fonts = fonts();
    let (record, asset_ids) = design(&fonts, 3, true);
    let previews = tempfile::tempdir().unwrap();
    let cache = RenderCache::new(previews.path().join("previews"));
    let cancel = Cancellation::new();
    let store = Store::new(Some((storage_key(asset_ids[1]), cancel.clone())));

    let outcome =
        application::renders::validate(account(), &record, None, &fonts, &Assets, &store, &cancel)
            .await;
    assert!(
        outcome.is_err(),
        "the run must actually have been cancelled"
    );

    for page in &record.document.pages {
        assert!(
            cache
                .get(record.id, record.version, page.id, 1.0, 0)
                .is_none(),
            "a cancelled validate left a cached render for {:?}",
            page.id
        );
    }
    assert_eq!(files_under(previews.path()), 0);

    // And the probe can see an entry when there is one, so the emptiness above
    // means something.
    let first = &record.document.pages[0];
    let loaded = application::renders::load_page_assets(
        &Assets,
        &store,
        account(),
        &record.document,
        first.id,
    )
    .await
    .unwrap();
    application::renders::render_png(
        &cache,
        record.id,
        record.version,
        &record.document,
        first.id,
        None,
        &fonts,
        &loaded,
        1.0,
    )
    .unwrap();
    assert!(cache
        .get(record.id, record.version, first.id, 1.0, 0)
        .is_some());
    assert!(files_under(previews.path()) > 0);
}

fn files_under(dir: &std::path::Path) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .map(|entry| {
            let path = entry.path();
            if path.is_dir() {
                files_under(&path)
            } else {
                1
            }
        })
        .sum()
}

/// A design with no images awaits nothing that pends, so before the
/// checkpoints yielded, validate ran every page inside one poll and a dropped
/// future had nowhere to land. Polled by hand here: it must hand control back
/// between pages, which is the only place a disconnect can stop it.
#[test]
fn validate_hands_control_back_between_pages_even_with_no_images() {
    let fonts = fonts();
    let (record, _) = design(&fonts, 3, false);
    let store = Store::new(None);
    let cancel = Cancellation::new();

    let mut validating = std::pin::pin!(application::renders::validate(
        account(),
        &record,
        None,
        &fonts,
        &Assets,
        &store,
        &cancel,
    ));
    let mut cx = Context::from_waker(Waker::noop());

    let mut polls = 0;
    let finished = loop {
        polls += 1;
        if let Poll::Ready(outcome) = validating.as_mut().poll(&mut cx) {
            break outcome;
        }
    };
    finished.expect("an uncancelled validate finishes");
    // One checkpoint before anything, then three per page.
    assert_eq!(polls, 1 + 1 + 3 * 3);

    // And a signal fired between two polls ends it at the next one.
    let mut validating = std::pin::pin!(application::renders::validate(
        account(),
        &record,
        None,
        &fonts,
        &Assets,
        &store,
        &cancel,
    ));
    assert!(validating.as_mut().poll(&mut cx).is_pending());
    cancel.cancel();
    match validating.as_mut().poll(&mut cx) {
        Poll::Ready(Err(error)) => assert_eq!(error.code(), "CANCELLED"),
        Poll::Ready(Ok(_)) => panic!("finished despite the signal"),
        Poll::Pending => panic!("did not stop at the next checkpoint"),
    }
}
