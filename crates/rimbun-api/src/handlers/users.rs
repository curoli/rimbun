use axum::{extract::State, http::HeaderMap, Json};
use serde::Serialize;

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
