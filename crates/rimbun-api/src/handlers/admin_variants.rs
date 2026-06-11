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
    db::{
        documents,
        sections,
        submissions::{self, NewSubmission},
        test_runs,
        users::{self, NewUser},
        variant_collections,
    },
    error::ApiError,
    http::extractors::require_current_user,
    state::AppState,
};

#[derive(Debug, Serialize)]
pub struct VariantCollectionDetail {
    pub collection: variant_collections::VariantCollectionRecord,
    pub entries: Vec<variant_collections::VariantEntryRecord>,
    pub runs: Vec<test_runs::TestRunRecord>,
}

#[derive(Debug, Deserialize)]
pub struct UpsertCollectionRequest {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Deserialize)]
pub struct UpsertEntryRequest {
    pub markdown_content: String,
}

#[derive(Debug, Serialize)]
pub struct RunCollectionResponse {
    pub run: test_runs::TestRunRecord,
    pub document: documents::DocumentRecord,
    pub section: sections::SectionRecord,
    pub created_users: usize,
}

fn require_admin(user: &users::UserRecord) -> Result<(), ApiError> {
    if matches!(user.role.as_str(), "privileged" | "admin") {
        Ok(())
    } else {
        Err(ApiError::forbidden("admin role required"))
    }
}

fn sanitize_username(seed: &str) -> String {
    let mut value = seed
        .chars()
        .flat_map(char::to_lowercase)
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect::<String>();
    value = value.trim_matches('_').to_owned();
    if value.is_empty() {
        "variant".to_owned()
    } else {
        value
    }
}

fn test_password_hash() -> Result<String, ApiError> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(b"rimbun-test-user", &salt)
        .map(|hash| hash.to_string())
        .map_err(|err| ApiError::internal(err.to_string()))
}

pub async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<VariantCollectionDetail>>, ApiError> {
    let user = require_current_user(State(state.clone()), &headers).await?;
    require_admin(&user)?;

    let collections = variant_collections::list_collections(&state.pool)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;

    let mut result = Vec::with_capacity(collections.len());
    for collection in collections {
        let entries = variant_collections::list_entries(&state.pool, collection.id)
            .await
            .map_err(|err| ApiError::internal(err.to_string()))?;
        let runs = test_runs::list_runs_by_collection(&state.pool, collection.id)
            .await
            .map_err(|err| ApiError::internal(err.to_string()))?;
        result.push(VariantCollectionDetail {
            collection,
            entries,
            runs,
        });
    }

    Ok(Json(result))
}

pub async fn create_collection(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<UpsertCollectionRequest>,
) -> Result<Json<variant_collections::VariantCollectionRecord>, ApiError> {
    let user = require_current_user(State(state.clone()), &headers).await?;
    require_admin(&user)?;

    if payload.name.trim().is_empty() {
        return Err(ApiError::bad_request("collection name is required"));
    }

    let collection = variant_collections::create_collection(
        &state.pool,
        &variant_collections::NewVariantCollection {
            id: uuid::Uuid::new_v4(),
            name: payload.name.trim().to_owned(),
            description: payload.description.trim().to_owned(),
            created_by: user.id,
        },
    )
    .await
    .map_err(|err| ApiError::bad_request(err.to_string()))?;

    Ok(Json(collection))
}

pub async fn update_collection(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(collection_id): Path<uuid::Uuid>,
    Json(payload): Json<UpsertCollectionRequest>,
) -> Result<Json<variant_collections::VariantCollectionRecord>, ApiError> {
    let user = require_current_user(State(state.clone()), &headers).await?;
    require_admin(&user)?;

    if payload.name.trim().is_empty() {
        return Err(ApiError::bad_request("collection name is required"));
    }

    let collection = variant_collections::update_collection(
        &state.pool,
        collection_id,
        payload.name.trim(),
        payload.description.trim(),
    )
    .await
    .map_err(|err| ApiError::internal(err.to_string()))?
    .ok_or_else(|| ApiError::not_found("collection not found"))?;

    Ok(Json(collection))
}

pub async fn delete_collection(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(collection_id): Path<uuid::Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let user = require_current_user(State(state.clone()), &headers).await?;
    require_admin(&user)?;

    if test_runs::has_active_runs(&state.pool, collection_id)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?
    {
        return Err(ApiError::bad_request(
            "cannot delete a collection while test runs are still active",
        ));
    }

    let deleted = variant_collections::delete_collection(&state.pool, collection_id)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;
    if !deleted {
        return Err(ApiError::not_found("collection not found"));
    }

    Ok(Json(serde_json::json!({ "status": "ok" })))
}

pub async fn create_entry(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(collection_id): Path<uuid::Uuid>,
    Json(payload): Json<UpsertEntryRequest>,
) -> Result<Json<variant_collections::VariantEntryRecord>, ApiError> {
    let user = require_current_user(State(state.clone()), &headers).await?;
    require_admin(&user)?;

    let _collection = variant_collections::find_collection_by_id(&state.pool, collection_id)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?
        .ok_or_else(|| ApiError::not_found("collection not found"))?;

    if payload.markdown_content.trim().is_empty() {
        return Err(ApiError::bad_request("markdown content is required"));
    }

    let existing_entries = variant_collections::list_entries(&state.pool, collection_id)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;
    let next_position = existing_entries
        .iter()
        .map(|entry| entry.position)
        .max()
        .map(|position| position + 1)
        .unwrap_or(0);
    let generated_label = format!("Variant {}", next_position + 1);
    let generated_username_hint = Some(format!("variant_{}", next_position + 1));

    let entry = variant_collections::create_entry(
        &state.pool,
        &variant_collections::NewVariantEntry {
            id: uuid::Uuid::new_v4(),
            collection_id,
            position: next_position,
            label: generated_label,
            username_hint: generated_username_hint,
            markdown_content: payload.markdown_content,
        },
    )
    .await
    .map_err(|err| ApiError::bad_request(err.to_string()))?;

    Ok(Json(entry))
}

pub async fn update_entry(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(entry_id): Path<uuid::Uuid>,
    Json(payload): Json<UpsertEntryRequest>,
) -> Result<Json<variant_collections::VariantEntryRecord>, ApiError> {
    let user = require_current_user(State(state.clone()), &headers).await?;
    require_admin(&user)?;

    if payload.markdown_content.trim().is_empty() {
        return Err(ApiError::bad_request("markdown content is required"));
    }

    let existing_entry = variant_collections::find_entry_by_id(&state.pool, entry_id)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?
        .ok_or_else(|| ApiError::not_found("entry not found"))?;

    let entry = variant_collections::update_entry(
        &state.pool,
        entry_id,
        existing_entry.position,
        &existing_entry.label,
        existing_entry.username_hint.as_deref(),
        &payload.markdown_content,
    )
    .await
    .map_err(|err| ApiError::bad_request(err.to_string()))?
    .ok_or_else(|| ApiError::not_found("entry not found"))?;

    Ok(Json(entry))
}

pub async fn delete_entry(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(entry_id): Path<uuid::Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let user = require_current_user(State(state.clone()), &headers).await?;
    require_admin(&user)?;

    let existing_entry = variant_collections::find_entry_by_id(&state.pool, entry_id)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?
        .ok_or_else(|| ApiError::not_found("entry not found"))?;

    let deleted = variant_collections::delete_entry(&state.pool, entry_id)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;
    if !deleted {
        return Err(ApiError::not_found("entry not found"));
    }

    variant_collections::normalize_entries(&state.pool, existing_entry.collection_id)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;

    Ok(Json(serde_json::json!({ "status": "ok" })))
}

pub async fn run_collection(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(collection_id): Path<uuid::Uuid>,
) -> Result<Json<RunCollectionResponse>, ApiError> {
    let user = require_current_user(State(state.clone()), &headers).await?;
    require_admin(&user)?;

    let collection = variant_collections::find_collection_by_id(&state.pool, collection_id)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?
        .ok_or_else(|| ApiError::not_found("collection not found"))?;
    let entries = variant_collections::list_entries(&state.pool, collection_id)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;

    if entries.is_empty() {
        return Err(ApiError::bad_request("collection must contain at least one variant"));
    }

    let run = test_runs::create_run(
        &state.pool,
        &test_runs::NewTestRun {
            id: uuid::Uuid::new_v4(),
            collection_id,
            created_by: user.id,
        },
    )
    .await
    .map_err(|err| ApiError::internal(err.to_string()))?;

    let document_slug = format!("test-{}-{}", sanitize_username(&collection.name), &run.id.to_string()[..8]);
    let document = documents::create(
        &state.pool,
        &documents::NewDocument {
            id: uuid::Uuid::new_v4(),
            slug: document_slug,
            title: format!("Test: {}", collection.name),
            visibility: "authenticated".to_owned(),
            markdown_policy: serde_json::json!({}),
            created_by: user.id,
        },
    )
    .await
    .map_err(|err| ApiError::bad_request(err.to_string()))?;

    let section_id = uuid::Uuid::new_v4();
    let section = sections::create(
        &state.pool,
        &sections::NewSection {
            id: section_id,
            document_id: document.id,
            parent_id: None,
            title: collection.name.clone(),
            has_heading: true,
            has_own_text: true,
            position: 0,
            path: section_id.to_string(),
        },
    )
    .await
    .map_err(|err| ApiError::bad_request(err.to_string()))?;

    let run = test_runs::attach_run_targets(&state.pool, run.id, document.id, section.id)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?
        .ok_or_else(|| ApiError::not_found("test run not found"))?;

    let password_hash = test_password_hash()?;
    for (index, entry) in entries.iter().enumerate() {
        let user_seed = entry.username_hint.as_deref().unwrap_or(&entry.label);
        let username = format!(
            "test_{}_{}_{}",
            &run.id.to_string()[..8],
            index + 1,
            sanitize_username(user_seed)
        );
        let test_user = users::create(
            &state.pool,
            &NewUser {
                id: uuid::Uuid::new_v4(),
                username,
                display_name: entry.label.clone(),
                email: format!("{}@rimbun.test", uuid::Uuid::new_v4()),
                password_hash: password_hash.clone(),
                role: "normal".to_owned(),
            },
        )
        .await
        .map_err(|err| ApiError::bad_request(err.to_string()))?;

        test_runs::create_run_user(&state.pool, run.id, test_user.id, entry.id)
            .await
            .map_err(|err| ApiError::internal(err.to_string()))?;

        let mut tx = state
            .pool
            .begin()
            .await
            .map_err(|err| ApiError::internal(err.to_string()))?;

        let submission = submissions::create(
            &mut tx,
            &NewSubmission {
                id: uuid::Uuid::new_v4(),
                section_id: section.id,
                user_id: test_user.id,
                base_submission_id: None,
                markdown_content: entry.markdown_content.clone(),
            },
        )
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;

        submissions::supersede_previous_active_for_user(&mut tx, section.id, test_user.id, submission.id)
            .await
            .map_err(|err| ApiError::internal(err.to_string()))?;

        tx.commit()
            .await
            .map_err(|err| ApiError::internal(err.to_string()))?;
    }

    crate::db::projections::rebuild_trivial_for_section(&state.pool, &state.embedding_client, section.id)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;

    Ok(Json(RunCollectionResponse {
        run,
        document,
        section,
        created_users: entries.len(),
    }))
}

pub async fn delete_run(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(run_id): Path<uuid::Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let user = require_current_user(State(state.clone()), &headers).await?;
    require_admin(&user)?;

    let run = test_runs::find_run_by_id(&state.pool, run_id)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?
        .ok_or_else(|| ApiError::not_found("test run not found"))?;

    let run_users = test_runs::list_run_users(&state.pool, run.id)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;

    if let Some(document_id) = run.document_id {
        let _ = documents::delete_by_id(&state.pool, document_id)
            .await
            .map_err(|err| ApiError::internal(err.to_string()))?;
    }

    for run_user in run_users {
        let _ = users::delete_by_id(&state.pool, run_user.user_id)
            .await
            .map_err(|err| ApiError::internal(err.to_string()))?;
    }

    let _ = test_runs::mark_run_deleted(&state.pool, run.id)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;

    Ok(Json(serde_json::json!({ "status": "ok" })))
}
