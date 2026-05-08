use axum::{
    extract::{Path, State},
    http::HeaderMap,
    Json,
};
use serde::Deserialize;

use crate::{
    db::{moderation, projections, submissions},
    error::ApiError,
    http::extractors::require_current_user,
    state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct UpdateModerationRequest {
    pub hidden: bool,
    pub soft_deleted: bool,
    pub excluded_from_clustering: bool,
    pub reason: Option<String>,
}

pub async fn update(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(submission_id): Path<uuid::Uuid>,
    Json(payload): Json<UpdateModerationRequest>,
) -> Result<Json<moderation::ModerationRecord>, ApiError> {
    let user = require_current_user(State(state.clone()), &headers).await?;
    if !matches!(user.role.as_str(), "privileged" | "admin") {
        return Err(ApiError::forbidden("admin role required"));
    }

    let submission = submissions::find_by_id(&state.pool, submission_id)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?
        .ok_or_else(|| ApiError::not_found("submission not found"))?;

    let moderation = moderation::upsert(
        &state.pool,
        &moderation::UpsertModeration {
            submission_id,
            hidden: payload.hidden,
            soft_deleted: payload.soft_deleted,
            excluded_from_clustering: payload.excluded_from_clustering,
            reason: payload.reason,
            moderated_by: user.id,
        },
    )
    .await
    .map_err(|err| ApiError::internal(err.to_string()))?;

    projections::rebuild_trivial_for_section(
        &state.pool,
        &state.embedding_client,
        submission.section_id,
    )
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;

    Ok(Json(moderation))
}
