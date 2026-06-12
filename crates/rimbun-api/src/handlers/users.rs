use argon2::{
    password_hash::{PasswordHasher, SaltString},
    Argon2,
};
use axum::{
    extract::{Path, State},
    http::HeaderMap,
    Json,
};
use rand_core::OsRng;
use serde::{Deserialize, Serialize};

use crate::{
    db::users,
    error::ApiError,
    http::extractors::require_current_user,
    state::AppState,
};

fn public_role(role: &str) -> &str {
    match role {
        "privileged" => "admin",
        other => other,
    }
}

#[derive(Debug, Serialize)]
pub struct UserListItem {
    pub id: uuid::Uuid,
    pub username: String,
    pub display_name: String,
    pub email: String,
    pub role: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl From<users::UserRecord> for UserListItem {
    fn from(value: users::UserRecord) -> Self {
        Self {
            id: value.id,
            username: value.username,
            display_name: value.display_name,
            email: value.email,
            role: public_role(&value.role).to_owned(),
            created_at: value.created_at,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ResetPasswordRequest {
    pub new_password: String,
}

pub async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<UserListItem>>, ApiError> {
    let user = require_current_user(State(state.clone()), &headers).await?;
    if !matches!(user.role.as_str(), "privileged" | "admin") {
        return Err(ApiError::forbidden("admin role required"));
    }

    let users = users::list_all(&state.pool)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;

    Ok(Json(users.into_iter().map(UserListItem::from).collect()))
}

pub async fn reset_password(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(user_id): Path<uuid::Uuid>,
    Json(payload): Json<ResetPasswordRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let user = require_current_user(State(state.clone()), &headers).await?;
    if !matches!(user.role.as_str(), "privileged" | "admin") {
        return Err(ApiError::forbidden("admin role required"));
    }

    if payload.new_password.len() < 8 {
        return Err(ApiError::bad_request(
            "new password must be at least 8 characters long",
        ));
    }

    let salt = SaltString::generate(&mut OsRng);
    let password_hash = Argon2::default()
        .hash_password(payload.new_password.as_bytes(), &salt)
        .map_err(|err| ApiError::internal(err.to_string()))?
        .to_string();

    users::update_password_hash(&state.pool, user_id, &password_hash)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?
        .ok_or_else(|| ApiError::not_found("user not found"))?;

    Ok(Json(serde_json::json!({ "status": "ok" })))
}
