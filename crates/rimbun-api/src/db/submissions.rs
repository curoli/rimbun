use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{FromRow, PgPool, Postgres, Transaction};

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct SubmissionRecord {
    pub id: uuid::Uuid,
    pub section_id: uuid::Uuid,
    pub user_id: uuid::Uuid,
    pub username: String,
    pub display_name: String,
    pub base_submission_id: Option<uuid::Uuid>,
    pub markdown_content: String,
    pub status: String,
    pub published_at: DateTime<Utc>,
    pub superseded_by: Option<uuid::Uuid>,
}

#[derive(Debug, Clone)]
pub struct NewSubmission {
    pub id: uuid::Uuid,
    pub section_id: uuid::Uuid,
    pub user_id: uuid::Uuid,
    pub base_submission_id: Option<uuid::Uuid>,
    pub markdown_content: String,
}

pub async fn create(
    tx: &mut Transaction<'_, Postgres>,
    submission: &NewSubmission,
) -> anyhow::Result<SubmissionRecord> {
    let record = sqlx::query_as::<_, SubmissionRecord>(
        r#"
        insert into submissions (id, section_id, user_id, base_submission_id, markdown_content, status)
        values ($1, $2, $3, $4, $5, 'published')
        returning
          id,
          section_id,
          user_id,
          (select username from users where id = user_id) as username,
          (select display_name from users where id = user_id) as display_name,
          base_submission_id,
          markdown_content,
          status,
          published_at,
          superseded_by
        "#,
    )
    .bind(submission.id)
    .bind(submission.section_id)
    .bind(submission.user_id)
    .bind(submission.base_submission_id)
    .bind(&submission.markdown_content)
    .fetch_one(&mut **tx)
    .await?;

    Ok(record)
}

pub async fn supersede_previous_active_for_user(
    tx: &mut Transaction<'_, Postgres>,
    section_id: uuid::Uuid,
    user_id: uuid::Uuid,
    new_submission_id: uuid::Uuid,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        update submissions
        set superseded_by = $3
        where section_id = $1
          and user_id = $2
          and id <> $3
          and superseded_by is null
        "#,
    )
    .bind(section_id)
    .bind(user_id)
    .bind(new_submission_id)
    .execute(&mut **tx)
    .await?;

    Ok(())
}

pub async fn list_by_section(
    pool: &PgPool,
    section_id: uuid::Uuid,
) -> anyhow::Result<Vec<SubmissionRecord>> {
    let records = sqlx::query_as::<_, SubmissionRecord>(
        r#"
        select
          s.id,
          s.section_id,
          s.user_id,
          u.username,
          u.display_name,
          s.base_submission_id,
          s.markdown_content,
          s.status,
          s.published_at,
          s.superseded_by
        from submissions s
        join users u on u.id = s.user_id
        where section_id = $1
        order by s.published_at desc
        "#,
    )
    .bind(section_id)
    .fetch_all(pool)
    .await?;

    Ok(records)
}

pub async fn list_active_by_section(
    pool: &PgPool,
    section_id: uuid::Uuid,
) -> anyhow::Result<Vec<SubmissionRecord>> {
    let records = sqlx::query_as::<_, SubmissionRecord>(
        r#"
        select
          s.id,
          s.section_id,
          s.user_id,
          u.username,
          u.display_name,
          s.base_submission_id,
          s.markdown_content,
          s.status,
          s.published_at,
          s.superseded_by
        from submissions s
        join users u on u.id = s.user_id
        where s.section_id = $1 and s.superseded_by is null
        order by s.published_at desc
        "#,
    )
    .bind(section_id)
    .fetch_all(pool)
    .await?;

    Ok(records)
}

pub async fn list_active_visible_by_section(
    pool: &PgPool,
    section_id: uuid::Uuid,
) -> anyhow::Result<Vec<SubmissionRecord>> {
    let records = sqlx::query_as::<_, SubmissionRecord>(
        r#"
        select
          s.id,
          s.section_id,
          s.user_id,
          u.username,
          u.display_name,
          s.base_submission_id,
          s.markdown_content,
          s.status,
          s.published_at,
          s.superseded_by
        from submissions s
        join users u on u.id = s.user_id
        left join submission_moderation sm on sm.submission_id = s.id
        where s.section_id = $1
          and s.superseded_by is null
          and coalesce(sm.soft_deleted, false) = false
          and coalesce(sm.hidden, false) = false
        order by s.published_at desc
        "#,
    )
    .bind(section_id)
    .fetch_all(pool)
    .await?;

    Ok(records)
}

pub async fn list_active_clusterable_visible_by_section(
    pool: &PgPool,
    section_id: uuid::Uuid,
) -> anyhow::Result<Vec<SubmissionRecord>> {
    let records = sqlx::query_as::<_, SubmissionRecord>(
        r#"
        select
          s.id,
          s.section_id,
          s.user_id,
          u.username,
          u.display_name,
          s.base_submission_id,
          s.markdown_content,
          s.status,
          s.published_at,
          s.superseded_by
        from submissions s
        join users u on u.id = s.user_id
        left join submission_moderation sm on sm.submission_id = s.id
        where s.section_id = $1
          and s.superseded_by is null
          and coalesce(sm.soft_deleted, false) = false
          and coalesce(sm.hidden, false) = false
          and coalesce(sm.excluded_from_clustering, false) = false
        order by s.published_at desc
        "#,
    )
    .bind(section_id)
    .fetch_all(pool)
    .await?;

    Ok(records)
}

pub async fn find_by_id(
    pool: &PgPool,
    submission_id: uuid::Uuid,
) -> anyhow::Result<Option<SubmissionRecord>> {
    let record = sqlx::query_as::<_, SubmissionRecord>(
        r#"
        select
          s.id,
          s.section_id,
          s.user_id,
          u.username,
          u.display_name,
          s.base_submission_id,
          s.markdown_content,
          s.status,
          s.published_at,
          s.superseded_by
        from submissions s
        join users u on u.id = s.user_id
        where s.id = $1
        "#,
    )
    .bind(submission_id)
    .fetch_optional(pool)
    .await?;

    Ok(record)
}
