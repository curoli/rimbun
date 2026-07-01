use axum::{
    Json,
    extract::{Path, State},
    http::HeaderMap,
};
use serde::{Deserialize, Serialize};

use crate::{
    db::{documents, drafts, preferences, projections, sections, submissions},
    error::ApiError,
    http::extractors::{maybe_current_user, require_current_user},
    state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct CreateSectionRequest {
    pub parent_id: Option<uuid::Uuid>,
    pub title: String,
    pub has_heading: bool,
    pub has_own_text: bool,
    pub position: i32,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSectionRequest {
    pub title: String,
    pub has_heading: bool,
    pub has_own_text: bool,
    pub position: i32,
    pub parent_id: Option<uuid::Uuid>,
}

#[derive(Debug, Serialize)]
pub struct SectionViewResponse {
    pub section: sections::SectionRecord,
    pub projection: Vec<projections::ProjectionItemRecord>,
    pub active_submissions: Vec<submissions::SubmissionRecord>,
    pub draft: Option<drafts::DraftRecord>,
    pub preferred_base_submission_id: Option<uuid::Uuid>,
}

pub async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(document_id): Path<uuid::Uuid>,
    Json(payload): Json<CreateSectionRequest>,
) -> Result<Json<sections::SectionRecord>, ApiError> {
    let user = require_current_user(State(state.clone()), &headers).await?;
    if user.role != "admin" {
        return Err(ApiError::forbidden("admin role required"));
    }

    let _document = documents::find_by_id(&state.pool, document_id)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?
        .ok_or_else(|| ApiError::not_found("document not found"))?;

    if payload.has_heading && payload.title.trim().is_empty() {
        return Err(ApiError::bad_request("title is required"));
    }

    let section_id = uuid::Uuid::new_v4();
    let path = if let Some(parent_id) = payload.parent_id {
        let parent = sections::find_by_id(&state.pool, parent_id)
            .await
            .map_err(|err| ApiError::internal(err.to_string()))?
            .ok_or_else(|| ApiError::not_found("parent section not found"))?;

        if parent.document_id != document_id {
            return Err(ApiError::bad_request(
                "parent section belongs to another document",
            ));
        }

        format!("{}/{}", parent.path, section_id)
    } else {
        section_id.to_string()
    };

    let section = sections::create(
        &state.pool,
        &sections::NewSection {
            id: section_id,
            document_id,
            parent_id: payload.parent_id,
            title: payload.title.trim().to_owned(),
            has_heading: payload.has_heading,
            has_own_text: payload.has_own_text,
            position: payload.position,
            path,
        },
    )
    .await
    .map_err(|err| ApiError::bad_request(err.to_string()))?;

    Ok(Json(section))
}

pub async fn show(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<uuid::Uuid>,
) -> Result<Json<sections::SectionRecord>, ApiError> {
    let current_user = maybe_current_user(State(state.clone()), &headers).await?;
    let section = sections::find_by_id(&state.pool, id)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?
        .ok_or_else(|| ApiError::not_found("section not found"))?;

    let document = documents::find_by_id(&state.pool, section.document_id)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?
        .ok_or_else(|| ApiError::not_found("document not found"))?;

    if document.visibility == "authenticated" && current_user.is_none() {
        return Err(ApiError::unauthorized("authentication required"));
    }

    Ok(Json(section))
}

pub async fn update(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<uuid::Uuid>,
    Json(payload): Json<UpdateSectionRequest>,
) -> Result<Json<sections::SectionRecord>, ApiError> {
    let user = require_current_user(State(state.clone()), &headers).await?;
    if user.role != "admin" {
        return Err(ApiError::forbidden("admin role required"));
    }

    let current = sections::find_by_id(&state.pool, id)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?
        .ok_or_else(|| ApiError::not_found("section not found"))?;

    if payload.has_heading && payload.title.trim().is_empty() {
        return Err(ApiError::bad_request("title is required"));
    }

    if let Some(parent_id) = payload.parent_id {
        if parent_id == id {
            return Err(ApiError::bad_request("a section cannot be its own parent"));
        }

        let parent = sections::find_by_id(&state.pool, parent_id)
            .await
            .map_err(|err| ApiError::internal(err.to_string()))?
            .ok_or_else(|| ApiError::not_found("parent section not found"))?;

        if parent.document_id != current.document_id {
            return Err(ApiError::bad_request(
                "parent section belongs to another document",
            ));
        }

        if parent.path == current.path || parent.path.starts_with(&(current.path.clone() + "/")) {
            return Err(ApiError::bad_request(
                "a section cannot move into its own subtree",
            ));
        }
    }

    let section = sections::move_section(
        &state.pool,
        id,
        payload.title.trim(),
        payload.has_heading,
        payload.has_own_text,
        payload.parent_id,
        payload.position,
    )
    .await
    .map_err(|err| ApiError::internal(err.to_string()))?
    .ok_or_else(|| ApiError::not_found("section not found"))?;

    Ok(Json(section))
}

pub async fn view(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(section_id): Path<uuid::Uuid>,
) -> Result<Json<SectionViewResponse>, ApiError> {
    let current_user = maybe_current_user(State(state.clone()), &headers).await?;
    let section = sections::find_by_id(&state.pool, section_id)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?
        .ok_or_else(|| ApiError::not_found("section not found"))?;

    let document = documents::find_by_id(&state.pool, section.document_id)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?
        .ok_or_else(|| ApiError::not_found("document not found"))?;

    if document.visibility == "authenticated" && current_user.is_none() {
        return Err(ApiError::unauthorized("authentication required"));
    }

    let projection = projections::list_by_section(&state.pool, section_id)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;
    let active_submissions = submissions::list_active_visible_by_section(&state.pool, section_id)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;

    let (draft, preferred_base_submission_id) = if let Some(user) = current_user {
        let draft = drafts::find_by_section_and_user(&state.pool, section_id, user.id)
            .await
            .map_err(|err| ApiError::internal(err.to_string()))?;
        let preference = preferences::find_by_user_and_section(&state.pool, user.id, section_id)
            .await
            .map_err(|err| ApiError::internal(err.to_string()))?;

        (
            draft,
            preference.map(|item| item.preferred_base_submission_id),
        )
    } else {
        (None, None)
    };

    Ok(Json(SectionViewResponse {
        section,
        projection,
        active_submissions,
        draft,
        preferred_base_submission_id,
    }))
}
