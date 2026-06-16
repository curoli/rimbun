use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{FromRow, PgPool};

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct TestRunRecord {
    pub id: uuid::Uuid,
    pub collection_id: uuid::Uuid,
    pub document_id: Option<uuid::Uuid>,
    pub section_id: Option<uuid::Uuid>,
    pub status: String,
    pub created_by: uuid::Uuid,
    pub created_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub deleted_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct NewTestRun {
    pub id: uuid::Uuid,
    pub collection_id: uuid::Uuid,
    pub created_by: uuid::Uuid,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct TestRunUserRecord {
    pub test_run_id: uuid::Uuid,
    pub user_id: uuid::Uuid,
    pub variant_entry_id: uuid::Uuid,
}

pub async fn list_runs(pool: &PgPool) -> anyhow::Result<Vec<TestRunRecord>> {
    let records = sqlx::query_as::<_, TestRunRecord>(
        r#"
        select id, collection_id, document_id, section_id, status, created_by, created_at, finished_at, deleted_at
        from test_runs
        order by created_at desc
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(records)
}

pub async fn list_runs_by_collection(
    pool: &PgPool,
    collection_id: uuid::Uuid,
) -> anyhow::Result<Vec<TestRunRecord>> {
    let records = sqlx::query_as::<_, TestRunRecord>(
        r#"
        select id, collection_id, document_id, section_id, status, created_by, created_at, finished_at, deleted_at
        from test_runs
        where collection_id = $1
        order by created_at desc
        "#,
    )
    .bind(collection_id)
    .fetch_all(pool)
    .await?;

    Ok(records)
}

pub async fn has_active_runs(pool: &PgPool, collection_id: uuid::Uuid) -> anyhow::Result<bool> {
    let result = sqlx::query_scalar::<_, i64>(
        r#"
        select count(*)::bigint
        from test_runs
        where collection_id = $1
          and status = 'active'
        "#,
    )
    .bind(collection_id)
    .fetch_one(pool)
    .await?;

    Ok(result > 0)
}

pub async fn find_run_by_id(
    pool: &PgPool,
    id: uuid::Uuid,
) -> anyhow::Result<Option<TestRunRecord>> {
    let record = sqlx::query_as::<_, TestRunRecord>(
        r#"
        select id, collection_id, document_id, section_id, status, created_by, created_at, finished_at, deleted_at
        from test_runs
        where id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    Ok(record)
}

pub async fn create_run(pool: &PgPool, run: &NewTestRun) -> anyhow::Result<TestRunRecord> {
    let record = sqlx::query_as::<_, TestRunRecord>(
        r#"
        insert into test_runs (id, collection_id, status, created_by)
        values ($1, $2, 'active', $3)
        returning id, collection_id, document_id, section_id, status, created_by, created_at, finished_at, deleted_at
        "#,
    )
    .bind(run.id)
    .bind(run.collection_id)
    .bind(run.created_by)
    .fetch_one(pool)
    .await?;

    Ok(record)
}

pub async fn attach_run_targets(
    pool: &PgPool,
    run_id: uuid::Uuid,
    document_id: uuid::Uuid,
    section_id: uuid::Uuid,
) -> anyhow::Result<Option<TestRunRecord>> {
    let record = sqlx::query_as::<_, TestRunRecord>(
        r#"
        update test_runs
        set document_id = $2, section_id = $3, finished_at = now()
        where id = $1
        returning id, collection_id, document_id, section_id, status, created_by, created_at, finished_at, deleted_at
        "#,
    )
    .bind(run_id)
    .bind(document_id)
    .bind(section_id)
    .fetch_optional(pool)
    .await?;

    Ok(record)
}

pub async fn mark_run_deleted(
    pool: &PgPool,
    run_id: uuid::Uuid,
) -> anyhow::Result<Option<TestRunRecord>> {
    let record = sqlx::query_as::<_, TestRunRecord>(
        r#"
        update test_runs
        set status = 'deleted', deleted_at = now(), finished_at = coalesce(finished_at, now())
        where id = $1
        returning id, collection_id, document_id, section_id, status, created_by, created_at, finished_at, deleted_at
        "#,
    )
    .bind(run_id)
    .fetch_optional(pool)
    .await?;

    Ok(record)
}

pub async fn create_run_user(
    pool: &PgPool,
    test_run_id: uuid::Uuid,
    user_id: uuid::Uuid,
    variant_entry_id: uuid::Uuid,
) -> anyhow::Result<TestRunUserRecord> {
    let record = sqlx::query_as::<_, TestRunUserRecord>(
        r#"
        insert into test_run_users (test_run_id, user_id, variant_entry_id)
        values ($1, $2, $3)
        returning test_run_id, user_id, variant_entry_id
        "#,
    )
    .bind(test_run_id)
    .bind(user_id)
    .bind(variant_entry_id)
    .fetch_one(pool)
    .await?;

    Ok(record)
}

pub async fn list_run_users(
    pool: &PgPool,
    test_run_id: uuid::Uuid,
) -> anyhow::Result<Vec<TestRunUserRecord>> {
    let records = sqlx::query_as::<_, TestRunUserRecord>(
        r#"
        select test_run_id, user_id, variant_entry_id
        from test_run_users
        where test_run_id = $1
        order by variant_entry_id asc
        "#,
    )
    .bind(test_run_id)
    .fetch_all(pool)
    .await?;

    Ok(records)
}
