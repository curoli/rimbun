use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{FromRow, PgPool};

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct PreferenceRecord {
    pub user_id: uuid::Uuid,
    pub section_id: uuid::Uuid,
    pub preferred_base_submission_id: uuid::Uuid,
    pub updated_at: DateTime<Utc>,
}

pub async fn upsert(
    pool: &PgPool,
    user_id: uuid::Uuid,
    section_id: uuid::Uuid,
    preferred_base_submission_id: uuid::Uuid,
) -> anyhow::Result<PreferenceRecord> {
    let record = sqlx::query_as::<_, PreferenceRecord>(
        r#"
        insert into user_section_preferences (user_id, section_id, preferred_base_submission_id)
        values ($1, $2, $3)
        on conflict (user_id, section_id)
        do update set
          preferred_base_submission_id = excluded.preferred_base_submission_id,
          updated_at = now()
        returning user_id, section_id, preferred_base_submission_id, updated_at
        "#,
    )
    .bind(user_id)
    .bind(section_id)
    .bind(preferred_base_submission_id)
    .fetch_one(pool)
    .await?;

    Ok(record)
}

pub async fn find_by_user_and_section(
    pool: &PgPool,
    user_id: uuid::Uuid,
    section_id: uuid::Uuid,
) -> anyhow::Result<Option<PreferenceRecord>> {
    let record = sqlx::query_as::<_, PreferenceRecord>(
        r#"
        select user_id, section_id, preferred_base_submission_id, updated_at
        from user_section_preferences
        where user_id = $1 and section_id = $2
        "#,
    )
    .bind(user_id)
    .bind(section_id)
    .fetch_optional(pool)
    .await?;

    Ok(record)
}
