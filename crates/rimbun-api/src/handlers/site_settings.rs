use axum::{extract::State, http::HeaderMap, Json};
use serde::Deserialize;

use crate::{
    db::{site_settings, users},
    error::ApiError,
    http::extractors::require_current_user,
    state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct UpdateSiteSettingsRequest {
    pub brand_name: String,
    pub browser_title: String,
}

fn require_admin(user: &users::UserRecord) -> Result<(), ApiError> {
    if matches!(user.role.as_str(), "privileged" | "admin") {
        Ok(())
    } else {
        Err(ApiError::forbidden("admin role required"))
    }
}

pub async fn get(
    State(state): State<AppState>,
) -> Result<Json<site_settings::SiteSettingsRecord>, ApiError> {
    let settings = site_settings::get(&state.pool)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;

    Ok(Json(settings))
}

pub async fn update(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<UpdateSiteSettingsRequest>,
) -> Result<Json<site_settings::SiteSettingsRecord>, ApiError> {
    let user = require_current_user(State(state.clone()), &headers).await?;
    require_admin(&user)?;

    if payload.brand_name.trim().is_empty() || payload.browser_title.trim().is_empty() {
        return Err(ApiError::bad_request("brand name and browser title are required"));
    }

    let settings = site_settings::upsert(
        &state.pool,
        payload.brand_name.trim(),
        payload.browser_title.trim(),
    )
    .await
    .map_err(|err| ApiError::bad_request(err.to_string()))?;

    Ok(Json(settings))
}
