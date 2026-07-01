use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
};
use axum::{Json, extract::State, http::HeaderMap};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use rand_core::OsRng;
use serde::{Deserialize, Serialize};

use crate::{
    db::{
        sessions::{self, NewSession},
        users::{self, NewUser, UserRecord},
    },
    error::ApiError,
    http::extractors::{
        SESSION_COOKIE_NAME, SESSION_HEADER_NAME, maybe_current_user, require_current_user,
    },
    state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    pub display_name: String,
    pub email: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub identifier: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateMeRequest {
    pub display_name: String,
}

#[derive(Debug, Deserialize)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

#[derive(Debug, Serialize)]
pub struct UserResponse {
    pub id: uuid::Uuid,
    pub username: String,
    pub display_name: String,
    pub email: String,
    pub role: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
pub struct AuthResponse {
    pub user: UserResponse,
    pub session_token: String,
}

impl From<UserRecord> for UserResponse {
    fn from(value: UserRecord) -> Self {
        Self {
            id: value.id,
            username: value.username,
            display_name: value.display_name,
            email: value.email,
            role: value.role,
            created_at: value.created_at,
        }
    }
}

fn session_cookie(token: &str) -> Cookie<'static> {
    Cookie::build((SESSION_COOKIE_NAME, token.to_owned()))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .build()
}

fn removal_cookie() -> Cookie<'static> {
    let mut cookie = Cookie::build((SESSION_COOKIE_NAME, ""))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .build();
    cookie.make_removal();
    cookie
}

pub async fn me(
    state: State<AppState>,
    headers: HeaderMap,
) -> Result<Json<UserResponse>, ApiError> {
    let user = require_current_user(state, &headers).await?;
    Ok(Json(user.into()))
}

pub async fn update_me(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<UpdateMeRequest>,
) -> Result<Json<UserResponse>, ApiError> {
    let user = require_current_user(State(state.clone()), &headers).await?;

    if payload.display_name.trim().is_empty() {
        return Err(ApiError::bad_request("display name is required"));
    }

    let updated = users::update_display_name(&state.pool, user.id, payload.display_name.trim())
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?
        .ok_or_else(|| ApiError::not_found("user not found"))?;

    Ok(Json(updated.into()))
}

pub async fn change_password(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<ChangePasswordRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let user = require_current_user(State(state.clone()), &headers).await?;

    if payload.new_password.len() < 8 {
        return Err(ApiError::bad_request(
            "new password must be at least 8 characters long",
        ));
    }

    let parsed_hash = PasswordHash::new(&user.password_hash)
        .map_err(|err| ApiError::internal(err.to_string()))?;

    Argon2::default()
        .verify_password(payload.current_password.as_bytes(), &parsed_hash)
        .map_err(|_| ApiError::unauthorized("current password is incorrect"))?;

    let salt = SaltString::generate(&mut OsRng);
    let password_hash = Argon2::default()
        .hash_password(payload.new_password.as_bytes(), &salt)
        .map_err(|err| ApiError::internal(err.to_string()))?
        .to_string();

    let _updated = users::update_password_hash(&state.pool, user.id, &password_hash)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?
        .ok_or_else(|| ApiError::not_found("user not found"))?;

    Ok(Json(serde_json::json!({ "status": "ok" })))
}

pub async fn register(
    State(state): State<AppState>,
    Json(payload): Json<RegisterRequest>,
) -> Result<(CookieJar, Json<AuthResponse>), ApiError> {
    if payload.username.trim().is_empty()
        || payload.display_name.trim().is_empty()
        || payload.email.trim().is_empty()
        || payload.password.len() < 8
    {
        return Err(ApiError::bad_request(
            "username, display name, email and a password of at least 8 characters are required",
        ));
    }

    let salt = SaltString::generate(&mut OsRng);
    let password_hash = Argon2::default()
        .hash_password(payload.password.as_bytes(), &salt)
        .map_err(|err| ApiError::internal(err.to_string()))?
        .to_string();

    let user = users::create(
        &state.pool,
        &NewUser {
            id: uuid::Uuid::new_v4(),
            username: payload.username.trim().to_owned(),
            display_name: payload.display_name.trim().to_owned(),
            email: payload.email.trim().to_owned(),
            password_hash,
            role: "normal".to_owned(),
        },
    )
    .await
    .map_err(|err| ApiError::bad_request(err.to_string()))?;

    let session = sessions::create(
        &state.pool,
        &NewSession {
            id: uuid::Uuid::new_v4(),
            token: uuid::Uuid::new_v4().to_string(),
            user_id: user.id,
        },
    )
    .await
    .map_err(|err| ApiError::internal(err.to_string()))?;

    let jar = CookieJar::new().add(session_cookie(&session.token));

    Ok((
        jar,
        Json(AuthResponse {
            user: user.into(),
            session_token: session.token,
        }),
    ))
}

pub async fn login(
    State(state): State<AppState>,
    Json(payload): Json<LoginRequest>,
) -> Result<(CookieJar, Json<AuthResponse>), ApiError> {
    let user = users::find_by_login_identifier(&state.pool, payload.identifier.trim())
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?
        .ok_or_else(|| ApiError::unauthorized("invalid credentials"))?;

    let parsed_hash = PasswordHash::new(&user.password_hash)
        .map_err(|err| ApiError::internal(err.to_string()))?;

    Argon2::default()
        .verify_password(payload.password.as_bytes(), &parsed_hash)
        .map_err(|_| ApiError::unauthorized("invalid credentials"))?;

    let session = sessions::create(
        &state.pool,
        &NewSession {
            id: uuid::Uuid::new_v4(),
            token: uuid::Uuid::new_v4().to_string(),
            user_id: user.id,
        },
    )
    .await
    .map_err(|err| ApiError::internal(err.to_string()))?;

    let jar = CookieJar::new().add(session_cookie(&session.token));

    Ok((
        jar,
        Json(AuthResponse {
            user: user.into(),
            session_token: session.token,
        }),
    ))
}

pub async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<(CookieJar, Json<serde_json::Value>), ApiError> {
    let _ = maybe_current_user(State(state.clone()), &headers).await?;

    let session_token = headers
        .get(SESSION_HEADER_NAME)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            headers
                .get(axum::http::header::COOKIE)
                .and_then(|value| value.to_str().ok())
                .and_then(|raw_cookie| {
                    raw_cookie
                        .split(';')
                        .filter_map(|part| part.trim().split_once('='))
                        .find(|(name, _)| *name == SESSION_COOKIE_NAME)
                        .map(|(_, token)| token.to_owned())
                })
        });

    if let Some(token) = session_token {
        sessions::delete_by_token(&state.pool, &token)
            .await
            .map_err(|err| ApiError::internal(err.to_string()))?;
    }

    let jar = CookieJar::new().add(removal_cookie());
    Ok((jar, Json(serde_json::json!({ "status": "ok" }))))
}
