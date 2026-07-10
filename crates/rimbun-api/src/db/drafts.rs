use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{FromRow, PgPool};

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct DraftRecord {
    pub id: uuid::Uuid,
    pub section_id: uuid::Uuid,
    pub user_id: uuid::Uuid,
    pub base_submission_id: Option<uuid::Uuid>,
    pub markdown_content: String,
    pub main_comment_markdown: Option<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct UpsertDraft {
    pub id: uuid::Uuid,
    pub section_id: uuid::Uuid,
    pub user_id: uuid::Uuid,
    pub base_submission_id: Option<uuid::Uuid>,
    pub markdown_content: String,
    pub main_comment_markdown: Option<String>,
}

pub async fn upsert(pool: &PgPool, draft: &UpsertDraft) -> anyhow::Result<DraftRecord> {
    let record = sqlx::query_as::<_, DraftRecord>(
        r#"
        insert into drafts (
          id,
          section_id,
          user_id,
          base_submission_id,
          markdown_content,
          main_comment_markdown
        )
        values ($1, $2, $3, $4, $5, $6)
        on conflict (section_id, user_id)
        do update set
          base_submission_id = excluded.base_submission_id,
          markdown_content = excluded.markdown_content,
          main_comment_markdown = excluded.main_comment_markdown,
          updated_at = now()
        returning id, section_id, user_id, base_submission_id, markdown_content, main_comment_markdown, updated_at
        "#,
    )
    .bind(draft.id)
    .bind(draft.section_id)
    .bind(draft.user_id)
    .bind(draft.base_submission_id)
    .bind(&draft.markdown_content)
    .bind(&draft.main_comment_markdown)
    .fetch_one(pool)
    .await?;

    Ok(record)
}

pub async fn find_by_section_and_user(
    pool: &PgPool,
    section_id: uuid::Uuid,
    user_id: uuid::Uuid,
) -> anyhow::Result<Option<DraftRecord>> {
    let record = sqlx::query_as::<_, DraftRecord>(
        r#"
        select id, section_id, user_id, base_submission_id, markdown_content, main_comment_markdown, updated_at
        from drafts
        where section_id = $1 and user_id = $2
        "#,
    )
    .bind(section_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?;

    Ok(record)
}
