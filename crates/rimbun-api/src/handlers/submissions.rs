use axum::{
    extract::{Path, State},
    http::HeaderMap,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::{
    db::{projections, sections, submissions},
    error::ApiError,
    http::extractors::{maybe_current_user, require_current_user},
    state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct PublishRequest {
    pub base_submission_id: Option<uuid::Uuid>,
    pub markdown_content: String,
}

#[derive(Debug, Serialize)]
pub struct PublishResponse {
    pub submission: submissions::SubmissionRecord,
    pub queued_jobs: Vec<String>,
}

pub async fn publish(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(section_id): Path<uuid::Uuid>,
    Json(payload): Json<PublishRequest>,
) -> Result<Json<PublishResponse>, ApiError> {
    let user = require_current_user(State(state.clone()), &headers).await?;

    let _section = sections::find_by_id(&state.pool, section_id)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?
        .ok_or_else(|| ApiError::not_found("section not found"))?;

    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;

    let submission = submissions::create(
        &mut tx,
        &submissions::NewSubmission {
            id: uuid::Uuid::new_v4(),
            section_id,
            user_id: user.id,
            base_submission_id: payload.base_submission_id,
            markdown_content: payload.markdown_content,
        },
    )
    .await
    .map_err(|err| ApiError::internal(err.to_string()))?;

    submissions::supersede_previous_active_for_user(&mut tx, section_id, user.id, submission.id)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;

    tx.commit()
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;

    projections::rebuild_trivial_for_section(&state.pool, &state.embedding_client, section_id)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;

    Ok(Json(PublishResponse {
        submission,
        queued_jobs: vec![
            "compute_embedding".to_owned(),
            "recompute_projection".to_owned(),
        ],
    }))
}

pub async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(section_id): Path<uuid::Uuid>,
) -> Result<Json<Vec<submissions::SubmissionRecord>>, ApiError> {
    let _ = maybe_current_user(State(state.clone()), &headers).await?;

    let _section = sections::find_by_id(&state.pool, section_id)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?
        .ok_or_else(|| ApiError::not_found("section not found"))?;

    let submissions = submissions::list_by_section(&state.pool, section_id)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;

    Ok(Json(submissions))
}
