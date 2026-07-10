use axum::{
    Json,
    extract::{Path, State},
    http::HeaderMap,
};
use serde::Deserialize;

use crate::{
    db::{drafts, sections},
    error::ApiError,
    http::extractors::require_current_user,
    state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct SaveDraftRequest {
    pub base_submission_id: Option<uuid::Uuid>,
    pub markdown_content: String,
    pub main_comment_markdown: Option<String>,
}

pub async fn save(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(section_id): Path<uuid::Uuid>,
    Json(payload): Json<SaveDraftRequest>,
) -> Result<Json<drafts::DraftRecord>, ApiError> {
    let user = require_current_user(State(state.clone()), &headers).await?;

    let section = sections::find_by_id(&state.pool, section_id)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?
        .ok_or_else(|| ApiError::not_found("section not found"))?;

    if !section.has_own_text {
        return Err(ApiError::bad_request(
            "this section has no own text and cannot store drafts",
        ));
    }

    let draft = drafts::upsert(
        &state.pool,
        &drafts::UpsertDraft {
            id: uuid::Uuid::new_v4(),
            section_id,
            user_id: user.id,
            base_submission_id: payload.base_submission_id,
            markdown_content: payload.markdown_content,
            main_comment_markdown: payload
                .main_comment_markdown
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty()),
        },
    )
    .await
    .map_err(|err| ApiError::internal(err.to_string()))?;

    Ok(Json(draft))
}
