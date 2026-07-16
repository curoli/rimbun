use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use serde::{Deserialize, Serialize};

use crate::{
    db::{comments, moderation, projections, sections, submissions},
    error::ApiError,
    http::extractors::{maybe_current_user, require_current_user},
    state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct PublishRequest {
    pub base_submission_id: Option<uuid::Uuid>,
    pub markdown_content: String,
    pub main_comment_markdown: Option<String>,
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

    let section = sections::find_by_id(&state.pool, section_id)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?
        .ok_or_else(|| ApiError::not_found("section not found"))?;

    if !section.has_own_text {
        return Err(ApiError::bad_request(
            "this section has no own text and cannot publish submissions",
        ));
    }

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

    let main_comment_markdown = payload
        .main_comment_markdown
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());

    if let Some(markdown_content) = main_comment_markdown {
        comments::create_in_tx(
            &mut tx,
            &comments::NewComment {
                id: uuid::Uuid::new_v4(),
                submission_id: submission.id,
                parent_comment_id: None,
                user_id: user.id,
                markdown_content,
                is_primary: true,
            },
        )
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;
    }

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

pub async fn delete(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(submission_id): Path<uuid::Uuid>,
) -> Result<StatusCode, ApiError> {
    let user = require_current_user(State(state.clone()), &headers).await?;
    let submission = submissions::find_by_id(&state.pool, submission_id)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?
        .ok_or_else(|| ApiError::not_found("submission not found"))?;

    if submission.user_id != user.id && user.role != "admin" {
        return Err(ApiError::forbidden(
            "only the submission author or an admin can delete this submission",
        ));
    }

    moderation::upsert(
        &state.pool,
        &moderation::UpsertModeration {
            submission_id,
            hidden: false,
            soft_deleted: true,
            excluded_from_clustering: false,
            reason: Some(if submission.user_id == user.id {
                "deleted by author".to_owned()
            } else {
                "deleted by admin".to_owned()
            }),
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

    Ok(StatusCode::NO_CONTENT)
}
