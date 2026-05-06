use axum::{extract::State, http::HeaderMap};

use crate::{
    db::{sessions, users},
    error::ApiError,
    state::AppState,
};

pub const SESSION_COOKIE_NAME: &str = "rimbun_session";
pub const SESSION_HEADER_NAME: &str = "x-rimbun-session";

fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    let raw_cookie = headers.get(axum::http::header::COOKIE)?.to_str().ok()?;

    raw_cookie.split(';').find_map(|part| {
        let trimmed = part.trim();
        let (key, value) = trimmed.split_once('=')?;
        if key == name {
            Some(value.to_owned())
        } else {
            None
        }
    })
}

fn session_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get(SESSION_HEADER_NAME)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| cookie_value(headers, SESSION_COOKIE_NAME))
}

pub async fn maybe_current_user(
    State(state): State<AppState>,
    headers: &HeaderMap,
) -> Result<Option<users::UserRecord>, ApiError> {
    let Some(session_token) = session_token(headers) else {
        return Ok(None);
    };

    let session = sessions::find_valid_by_token(&state.pool, &session_token)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;
    let Some(session) = session else {
        return Ok(None);
    };

    let user = users::find_by_id(&state.pool, session.user_id)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;

    Ok(user)
}

pub async fn require_current_user(
    State(state): State<AppState>,
    headers: &HeaderMap,
) -> Result<users::UserRecord, ApiError> {
    maybe_current_user(State(state), headers)
        .await?
        .ok_or_else(|| ApiError::unauthorized("authentication required"))
}
