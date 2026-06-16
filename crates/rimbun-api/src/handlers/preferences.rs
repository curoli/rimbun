use axum::{
    Json,
    extract::{Path, State},
    http::HeaderMap,
};
use serde::Deserialize;

use crate::{
    db::{preferences, sections, submissions},
    error::ApiError,
    http::extractors::require_current_user,
    state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct SetBaseRequest {
    pub preferred_base_submission_id: uuid::Uuid,
}

pub async fn set_base(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(section_id): Path<uuid::Uuid>,
    Json(payload): Json<SetBaseRequest>,
) -> Result<Json<preferences::PreferenceRecord>, ApiError> {
    let user = require_current_user(State(state.clone()), &headers).await?;

    let _section = sections::find_by_id(&state.pool, section_id)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?
        .ok_or_else(|| ApiError::not_found("section not found"))?;

    let submission = submissions::find_by_id(&state.pool, payload.preferred_base_submission_id)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?
        .ok_or_else(|| ApiError::not_found("submission not found"))?;

    if submission.section_id != section_id {
        return Err(ApiError::bad_request(
            "submission belongs to another section",
        ));
    }

    let preference = preferences::upsert(
        &state.pool,
        user.id,
        section_id,
        payload.preferred_base_submission_id,
    )
    .await
    .map_err(|err| ApiError::internal(err.to_string()))?;

    Ok(Json(preference))
}
