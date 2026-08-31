//! Assets: upload, listing, download and soft delete (plan section 31).

use actix_multipart::Multipart;
use actix_web::{delete, get, post, web, HttpResponse};
use design_core::AssetId;
use futures_util::StreamExt as _;

use crate::error::ApiError;
use crate::routes::{parse_id, project_id};
use crate::state::AppState;

fn asset_id(raw: &str) -> Result<AssetId, ApiError> {
    Ok(AssetId(parse_id(raw, "asset id")?))
}

/// Reads the first file field, refusing anything over the limit as it arrives.
///
/// The cap is enforced while reading rather than after: buffering an
/// unbounded upload and only then measuring it is how one request takes the
/// process down. `actix-multipart` is configured without its tempfile store for
/// the same reason — nothing reaches disk before the magic-byte check has run.
async fn read_upload(
    mut payload: Multipart,
    max_bytes: usize,
) -> Result<(Vec<u8>, Option<String>), ApiError> {
    while let Some(item) = payload.next().await {
        let mut field = item.map_err(|e| {
            ApiError::bad_request(
                "MALFORMED_BODY",
                format!("the multipart body could not be read: {e}"),
                "Send the file as multipart/form-data with a single file field.",
            )
        })?;

        let filename = field.content_disposition().and_then(|d| d.get_filename());
        // The name is only ever metadata: what the bytes are is decided by
        // their content, never by this (plan section 44).
        let filename = filename.map(str::to_string);

        let mut bytes = Vec::new();
        while let Some(chunk) = field.next().await {
            let chunk = chunk.map_err(|e| {
                ApiError::bad_request(
                    "MALFORMED_BODY",
                    format!("the upload stream broke: {e}"),
                    "Retry the upload.",
                )
            })?;

            if bytes.len() + chunk.len() > max_bytes {
                return Err(ApiError::payload_too_large(
                    format!("the upload exceeds the {max_bytes} byte limit"),
                    "Resize or recompress the file and upload it again.",
                ));
            }
            bytes.extend_from_slice(&chunk);
        }

        if !bytes.is_empty() {
            return Ok((bytes, filename));
        }
    }

    Err(ApiError::bad_request(
        "MALFORMED_BODY",
        "the request carried no file",
        "Send the file as multipart/form-data with a single file field.",
    ))
}

#[post("/projects/{id}/assets")]
async fn upload(
    state: web::Data<AppState>,
    path: web::Path<String>,
    payload: Multipart,
) -> Result<HttpResponse, ApiError> {
    let project = project_id(&path)?;
    let (bytes, filename) = read_upload(payload, state.max_upload_bytes).await?;

    let (asset, created) = application::assets::upload(
        &*state.projects,
        &*state.assets,
        &*state.store,
        crate::routes::auth(&state).account_id,
        project,
        bytes,
        filename,
        state.max_upload_bytes,
    )
    .await?;

    // 200 rather than 201 when deduplication returned an existing asset: the
    // caller gets a usable asset_id either way, and the status says whether
    // anything new was actually stored (plan section 73).
    let mut response = if created {
        HttpResponse::Created()
    } else {
        HttpResponse::Ok()
    };
    Ok(response.json(asset))
}

#[get("/projects/{id}/assets")]
async fn list(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    let account = crate::routes::auth(&state).account_id;
    let project = project_id(&path)?;
    application::projects::get(&*state.projects, account, project).await?;
    let assets = application::assets::list(&*state.assets, account, project).await?;
    Ok(HttpResponse::Ok().json(assets))
}

#[get("/assets/{id}")]
async fn get(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    let asset = application::assets::get(
        &*state.assets,
        crate::routes::auth(&state).account_id,
        asset_id(&path)?,
    )
    .await?;
    Ok(HttpResponse::Ok().json(asset))
}

/// The bytes themselves, for an `<img>` in the editor.
#[get("/assets/{id}/content")]
async fn content(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    let (asset, bytes) = application::assets::bytes(
        &*state.assets,
        &*state.store,
        crate::routes::auth(&state).account_id,
        asset_id(&path)?,
    )
    .await?;

    Ok(HttpResponse::Ok()
        .content_type(asset.mime_type.as_str())
        // An asset is addressed by a content hash in all but name: the bytes
        // behind one id never change.
        .insert_header(("cache-control", "private, max-age=31536000, immutable"))
        .body(bytes))
}

/// A soft delete: historical design versions may still reference the asset, so
/// the bytes stay put and real garbage collection is deferred (plan section 74).
#[delete("/assets/{id}")]
async fn remove(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    application::assets::delete(
        &*state.assets,
        crate::routes::auth(&state).account_id,
        asset_id(&path)?,
    )
    .await?;
    Ok(HttpResponse::NoContent().finish())
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(upload)
        .service(list)
        .service(get)
        .service(content)
        .service(remove);
}
