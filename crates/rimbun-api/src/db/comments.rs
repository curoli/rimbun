use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool, Postgres, Transaction};

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct CommentRecord {
    pub id: uuid::Uuid,
    pub submission_id: uuid::Uuid,
    pub parent_comment_id: Option<uuid::Uuid>,
    pub user_id: uuid::Uuid,
    pub username: String,
    pub display_name: String,
    pub markdown_content: String,
    pub is_primary: bool,
    pub created_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub deleted_by: Option<uuid::Uuid>,
}

#[derive(Debug, Clone)]
pub struct NewComment {
    pub id: uuid::Uuid,
    pub submission_id: uuid::Uuid,
    pub parent_comment_id: Option<uuid::Uuid>,
    pub user_id: uuid::Uuid,
    pub markdown_content: String,
    pub is_primary: bool,
}

async fn create_with_executor<'e, E>(
    executor: E,
    comment: &NewComment,
) -> anyhow::Result<CommentRecord>
where
    E: sqlx::Executor<'e, Database = Postgres>,
{
    let record = sqlx::query_as::<_, CommentRecord>(
        r#"
        insert into comments (
          id,
          submission_id,
          parent_comment_id,
          user_id,
          markdown_content,
          is_primary
        )
        values ($1, $2, $3, $4, $5, $6)
        returning
          id,
          submission_id,
          parent_comment_id,
          user_id,
          (select username from users where id = user_id) as username,
          (select display_name from users where id = user_id) as display_name,
          markdown_content,
          is_primary,
          created_at,
          deleted_at,
          deleted_by
        "#,
    )
    .bind(comment.id)
    .bind(comment.submission_id)
    .bind(comment.parent_comment_id)
    .bind(comment.user_id)
    .bind(&comment.markdown_content)
    .bind(comment.is_primary)
    .fetch_one(executor)
    .await?;

    Ok(record)
}

pub async fn create(pool: &PgPool, comment: &NewComment) -> anyhow::Result<CommentRecord> {
    create_with_executor(pool, comment).await
}

pub async fn create_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    comment: &NewComment,
) -> anyhow::Result<CommentRecord> {
    create_with_executor(&mut **tx, comment).await
}

pub async fn find_by_id(
    pool: &PgPool,
    comment_id: uuid::Uuid,
) -> anyhow::Result<Option<CommentRecord>> {
    let record = sqlx::query_as::<_, CommentRecord>(
        r#"
        select
          c.id,
          c.submission_id,
          c.parent_comment_id,
          c.user_id,
          u.username,
          u.display_name,
          case when c.deleted_at is null then c.markdown_content else '' end as markdown_content,
          c.is_primary,
          c.created_at,
          c.deleted_at,
          c.deleted_by
        from comments c
        join users u on u.id = c.user_id
        where c.id = $1
        "#,
    )
    .bind(comment_id)
    .fetch_optional(pool)
    .await?;

    Ok(record)
}

pub async fn list_by_submission(
    pool: &PgPool,
    submission_id: uuid::Uuid,
) -> anyhow::Result<Vec<CommentRecord>> {
    let records = sqlx::query_as::<_, CommentRecord>(
        r#"
        select
          c.id,
          c.submission_id,
          c.parent_comment_id,
          c.user_id,
          u.username,
          u.display_name,
          case when c.deleted_at is null then c.markdown_content else '' end as markdown_content,
          c.is_primary,
          c.created_at,
          c.deleted_at,
          c.deleted_by
        from comments c
        join users u on u.id = c.user_id
        where c.submission_id = $1
        order by c.is_primary desc, c.created_at asc
        "#,
    )
    .bind(submission_id)
    .fetch_all(pool)
    .await?;

    Ok(records)
}

pub async fn list_by_submission_ids(
    pool: &PgPool,
    submission_ids: &[uuid::Uuid],
) -> anyhow::Result<Vec<CommentRecord>> {
    if submission_ids.is_empty() {
        return Ok(Vec::new());
    }

    let records = sqlx::query_as::<_, CommentRecord>(
        r#"
        select
          c.id,
          c.submission_id,
          c.parent_comment_id,
          c.user_id,
          u.username,
          u.display_name,
          case when c.deleted_at is null then c.markdown_content else '' end as markdown_content,
          c.is_primary,
          c.created_at,
          c.deleted_at,
          c.deleted_by
        from comments c
        join users u on u.id = c.user_id
        where c.submission_id = any($1)
        order by c.submission_id asc, c.is_primary desc, c.created_at asc
        "#,
    )
    .bind(submission_ids)
    .fetch_all(pool)
    .await?;

    Ok(records)
}

pub async fn find_primary_for_submission_and_user(
    pool: &PgPool,
    submission_id: uuid::Uuid,
    user_id: uuid::Uuid,
) -> anyhow::Result<Option<CommentRecord>> {
    let record = sqlx::query_as::<_, CommentRecord>(
        r#"
        select
          c.id,
          c.submission_id,
          c.parent_comment_id,
          c.user_id,
          u.username,
          u.display_name,
          case when c.deleted_at is null then c.markdown_content else '' end as markdown_content,
          c.is_primary,
          c.created_at,
          c.deleted_at,
          c.deleted_by
        from comments c
        join users u on u.id = c.user_id
        where c.submission_id = $1
          and c.user_id = $2
          and c.is_primary = true
          and c.parent_comment_id is null
          and c.deleted_at is null
        "#,
    )
    .bind(submission_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?;

    Ok(record)
}

pub async fn soft_delete(
    pool: &PgPool,
    comment_id: uuid::Uuid,
    deleted_by: uuid::Uuid,
) -> anyhow::Result<bool> {
    let result = sqlx::query(
        r#"
        update comments
        set markdown_content = '', deleted_at = now(), deleted_by = $2
        where id = $1 and deleted_at is null
        "#,
    )
    .bind(comment_id)
    .bind(deleted_by)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}
