use axum::{
    Json,
    extract::{Path, State},
    http::HeaderMap,
};
use serde::{Deserialize, Serialize};

use crate::{
    db::{documents, sections},
    error::ApiError,
    http::extractors::{maybe_current_user, require_current_user},
    state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct CreateDocumentRequest {
    pub slug: String,
    pub title: String,
    pub visibility: String,
    pub markdown_policy: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateDocumentRequest {
    pub slug: String,
    pub title: String,
    pub visibility: String,
    pub markdown_policy: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct DocumentDetailResponse {
    pub document: documents::DocumentRecord,
    pub sections: Vec<sections::SectionRecord>,
}

pub async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<documents::DocumentRecord>>, ApiError> {
    let user = maybe_current_user(State(state.clone()), &headers).await?;
    let documents = documents::list_visible(&state.pool, user.is_some())
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;

    Ok(Json(documents))
}

pub async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<CreateDocumentRequest>,
) -> Result<Json<documents::DocumentRecord>, ApiError> {
    let user = require_current_user(State(state.clone()), &headers).await?;

    if !matches!(user.role.as_str(), "privileged" | "admin") {
        return Err(ApiError::forbidden("admin role required"));
    }

    if payload.slug.trim().is_empty() || payload.title.trim().is_empty() {
        return Err(ApiError::bad_request("slug and title are required"));
    }

    if !matches!(payload.visibility.as_str(), "public" | "authenticated") {
        return Err(ApiError::bad_request(
            "visibility must be public or authenticated",
        ));
    }

    let document = documents::create(
        &state.pool,
        &documents::NewDocument {
            id: uuid::Uuid::new_v4(),
            slug: payload.slug.trim().to_owned(),
            title: payload.title.trim().to_owned(),
            visibility: payload.visibility,
            markdown_policy: payload
                .markdown_policy
                .unwrap_or_else(|| serde_json::json!({})),
            created_by: user.id,
        },
    )
    .await
    .map_err(|err| ApiError::bad_request(err.to_string()))?;

    Ok(Json(document))
}

pub async fn show(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<uuid::Uuid>,
) -> Result<Json<DocumentDetailResponse>, ApiError> {
    let current_user = maybe_current_user(State(state.clone()), &headers).await?;
    let document = documents::find_by_id(&state.pool, id)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?
        .ok_or_else(|| ApiError::not_found("document not found"))?;

    if document.visibility == "authenticated" && current_user.is_none() {
        return Err(ApiError::unauthorized("authentication required"));
    }

    let sections = sections::list_by_document(&state.pool, document.id)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;

    Ok(Json(DocumentDetailResponse { document, sections }))
}

pub async fn update(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<uuid::Uuid>,
    Json(payload): Json<UpdateDocumentRequest>,
) -> Result<Json<documents::DocumentRecord>, ApiError> {
    let user = require_current_user(State(state.clone()), &headers).await?;

    if !matches!(user.role.as_str(), "privileged" | "admin") {
        return Err(ApiError::forbidden("admin role required"));
    }

    if payload.slug.trim().is_empty() || payload.title.trim().is_empty() {
        return Err(ApiError::bad_request("slug and title are required"));
    }

    if !matches!(payload.visibility.as_str(), "public" | "authenticated") {
        return Err(ApiError::bad_request(
            "visibility must be public or authenticated",
        ));
    }

    let document = documents::update(
        &state.pool,
        id,
        payload.slug.trim(),
        payload.title.trim(),
        &payload.visibility,
        &payload
            .markdown_policy
            .unwrap_or_else(|| serde_json::json!({})),
    )
    .await
    .map_err(|err| ApiError::bad_request(err.to_string()))?
    .ok_or_else(|| ApiError::not_found("document not found"))?;

    Ok(Json(document))
}
