use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{FromRow, PgPool};

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct ModerationRecord {
    pub submission_id: uuid::Uuid,
    pub hidden: bool,
    pub soft_deleted: bool,
    pub excluded_from_clustering: bool,
    pub reason: Option<String>,
    pub moderated_by: Option<uuid::Uuid>,
    pub moderated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct UpsertModeration {
    pub submission_id: uuid::Uuid,
    pub hidden: bool,
    pub soft_deleted: bool,
    pub excluded_from_clustering: bool,
    pub reason: Option<String>,
    pub moderated_by: uuid::Uuid,
}

pub async fn upsert(
    pool: &PgPool,
    moderation: &UpsertModeration,
) -> anyhow::Result<ModerationRecord> {
    let record = sqlx::query_as::<_, ModerationRecord>(
        r#"
        insert into submission_moderation (
          submission_id,
          hidden,
          soft_deleted,
          excluded_from_clustering,
          reason,
          moderated_by,
          moderated_at
        )
        values ($1, $2, $3, $4, $5, $6, now())
        on conflict (submission_id)
        do update set
          hidden = excluded.hidden,
          soft_deleted = excluded.soft_deleted,
          excluded_from_clustering = excluded.excluded_from_clustering,
          reason = excluded.reason,
          moderated_by = excluded.moderated_by,
          moderated_at = now()
        returning submission_id, hidden, soft_deleted, excluded_from_clustering, reason, moderated_by, moderated_at
        "#,
    )
    .bind(moderation.submission_id)
    .bind(moderation.hidden)
    .bind(moderation.soft_deleted)
    .bind(moderation.excluded_from_clustering)
    .bind(&moderation.reason)
    .bind(moderation.moderated_by)
    .fetch_one(pool)
    .await?;

    Ok(record)
}

pub async fn find_by_submission_id(
    pool: &PgPool,
    submission_id: uuid::Uuid,
) -> anyhow::Result<Option<ModerationRecord>> {
    let record = sqlx::query_as::<_, ModerationRecord>(
        r#"
        select submission_id, hidden, soft_deleted, excluded_from_clustering, reason, moderated_by, moderated_at
        from submission_moderation
        where submission_id = $1
        "#,
    )
    .bind(submission_id)
    .fetch_optional(pool)
    .await?;

    Ok(record)
}
