//! Personal access token lifecycle (plan §112.1).
//!
//! The account comes only from `AuthContext`; no request body can choose it.
//! A creation response carries the secret once, while every later response is
//! safe metadata with no hash and no complete token.

use actix_web::{delete, get, post, web, HttpRequest, HttpResponse};
use design_core::ApiKeyId;
use persistence::ApiKeyRepository;
use serde::Deserialize;
use ts_rs::TS;

use crate::error::ApiError;
use crate::routes::{auth, parse_id};
use crate::state::AppState;

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct CreateApiKeyRequest {
    pub name: String,
}

fn api_key_id(raw: &str) -> Result<ApiKeyId, ApiError> {
    Ok(ApiKeyId(parse_id(raw, "API key id")?))
}

#[post("/api-keys")]
async fn create(
    state: web::Data<AppState>,
    request: HttpRequest,
    body: web::Json<CreateApiKeyRequest>,
) -> Result<HttpResponse, ApiError> {
    let name = body.name.trim();
    if name.is_empty() {
        return Err(ApiError::bad_request(
            "INVALID_BODY",
            "an API key needs a name",
            "Name the client or machine that will use this key.",
        ));
    }

    let auth = auth(&request, &state);
    let created = state
        .api_keys
        .create(auth.account_id, auth.user_id, name)
        .await
        .map_err(|error| ApiError::Application(error.into()))?;
    Ok(HttpResponse::Created().json(created))
}

#[get("/api-keys")]
async fn list(state: web::Data<AppState>, request: HttpRequest) -> Result<HttpResponse, ApiError> {
    let keys = state
        .api_keys
        .list(auth(&request, &state).account_id)
        .await
        .map_err(|error| ApiError::Application(error.into()))?;
    Ok(HttpResponse::Ok().json(keys))
}

#[delete("/api-keys/{id}")]
async fn revoke(
    state: web::Data<AppState>,
    request: HttpRequest,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    state
        .api_keys
        .revoke(auth(&request, &state).account_id, api_key_id(&path)?)
        .await
        .map_err(|error| ApiError::Application(error.into()))?;
    Ok(HttpResponse::NoContent().finish())
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(create).service(list).service(revoke);
}
