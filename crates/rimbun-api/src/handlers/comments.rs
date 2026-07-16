use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use serde::Deserialize;

use crate::{
    db::{comments, documents, moderation, sections, submissions},
    error::ApiError,
    http::extractors::{maybe_current_user, require_current_user},
    state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct CreateCommentRequest {
    pub parent_comment_id: Option<uuid::Uuid>,
    pub markdown_content: String,
    pub is_primary: Option<bool>,
}

async fn require_visible_submission(
    state: &AppState,
    submission_id: uuid::Uuid,
) -> Result<(), ApiError> {
    let moderation = moderation::find_by_submission_id(&state.pool, submission_id)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;
    if moderation.is_some_and(|record| record.hidden || record.soft_deleted) {
        return Err(ApiError::not_found("submission not found"));
    }
    Ok(())
}

pub async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(submission_id): Path<uuid::Uuid>,
) -> Result<Json<Vec<comments::CommentRecord>>, ApiError> {
    let current_user = maybe_current_user(State(state.clone()), &headers).await?;

    let submission = submissions::find_by_id(&state.pool, submission_id)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?
        .ok_or_else(|| ApiError::not_found("submission not found"))?;
    require_visible_submission(&state, submission_id).await?;

    let section = sections::find_by_id(&state.pool, submission.section_id)
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

    let comments = comments::list_by_submission(&state.pool, submission_id)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;

    Ok(Json(comments))
}

pub async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(submission_id): Path<uuid::Uuid>,
    Json(payload): Json<CreateCommentRequest>,
) -> Result<Json<comments::CommentRecord>, ApiError> {
    let user = require_current_user(State(state.clone()), &headers).await?;

    let submission = submissions::find_by_id(&state.pool, submission_id)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?
        .ok_or_else(|| ApiError::not_found("submission not found"))?;
    require_visible_submission(&state, submission_id).await?;

    let markdown_content = payload.markdown_content.trim();
    if markdown_content.is_empty() {
        return Err(ApiError::bad_request("comment text is required"));
    }

    if let Some(parent_comment_id) = payload.parent_comment_id {
        let parent = comments::find_by_id(&state.pool, parent_comment_id)
            .await
            .map_err(|err| ApiError::internal(err.to_string()))?
            .ok_or_else(|| ApiError::not_found("parent comment not found"))?;
        if parent.submission_id != submission_id {
            return Err(ApiError::bad_request(
                "parent comment belongs to another submission",
            ));
        }
    }

    let is_primary = payload.is_primary.unwrap_or(false);
    if is_primary {
        if payload.parent_comment_id.is_some() {
            return Err(ApiError::bad_request("a primary comment cannot be a reply"));
        }
        if submission.user_id != user.id {
            return Err(ApiError::forbidden(
                "only the submission author can create the primary comment",
            ));
        }
        if comments::find_primary_for_submission_and_user(&state.pool, submission_id, user.id)
            .await
            .map_err(|err| ApiError::internal(err.to_string()))?
            .is_some()
        {
            return Err(ApiError::bad_request(
                "a primary comment already exists for this submission",
            ));
        }
    }

    let comment = comments::create(
        &state.pool,
        &comments::NewComment {
            id: uuid::Uuid::new_v4(),
            submission_id,
            parent_comment_id: payload.parent_comment_id,
            user_id: user.id,
            markdown_content: markdown_content.to_owned(),
            is_primary,
        },
    )
    .await
    .map_err(|err| ApiError::internal(err.to_string()))?;

    Ok(Json(comment))
}

pub async fn delete(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(comment_id): Path<uuid::Uuid>,
) -> Result<StatusCode, ApiError> {
    let user = require_current_user(State(state.clone()), &headers).await?;
    let comment = comments::find_by_id(&state.pool, comment_id)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?
        .ok_or_else(|| ApiError::not_found("comment not found"))?;

    if comment.user_id != user.id && user.role != "admin" {
        return Err(ApiError::forbidden(
            "only the comment author or an admin can delete this comment",
        ));
    }

    comments::soft_delete(&state.pool, comment_id, user.id)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;

    Ok(StatusCode::NO_CONTENT)
}
