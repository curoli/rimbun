use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct DocumentRecord {
    pub id: uuid::Uuid,
    pub slug: String,
    pub title: String,
    pub visibility: String,
    pub markdown_policy: serde_json::Value,
    pub created_by: uuid::Uuid,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewDocument {
    pub id: uuid::Uuid,
    pub slug: String,
    pub title: String,
    pub visibility: String,
    pub markdown_policy: serde_json::Value,
    pub created_by: uuid::Uuid,
}

pub async fn create(pool: &PgPool, document: &NewDocument) -> anyhow::Result<DocumentRecord> {
    let record = sqlx::query_as::<_, DocumentRecord>(
        r#"
        insert into documents (id, slug, title, visibility, markdown_policy, created_by)
        values ($1, $2, $3, $4, $5, $6)
        returning id, slug, title, visibility, markdown_policy, created_by, created_at
        "#,
    )
    .bind(document.id)
    .bind(&document.slug)
    .bind(&document.title)
    .bind(&document.visibility)
    .bind(&document.markdown_policy)
    .bind(document.created_by)
    .fetch_one(pool)
    .await?;

    Ok(record)
}

pub async fn list_visible(
    pool: &PgPool,
    include_authenticated: bool,
) -> anyhow::Result<Vec<DocumentRecord>> {
    let visibility = if include_authenticated {
        vec!["public", "authenticated"]
    } else {
        vec!["public"]
    };

    let records = sqlx::query_as::<_, DocumentRecord>(
        r#"
        select id, slug, title, visibility, markdown_policy, created_by, created_at
        from documents
        where visibility = any($1)
        order by created_at asc
        "#,
    )
    .bind(&visibility)
    .fetch_all(pool)
    .await?;

    Ok(records)
}

pub async fn find_by_id(pool: &PgPool, id: uuid::Uuid) -> anyhow::Result<Option<DocumentRecord>> {
    let record = sqlx::query_as::<_, DocumentRecord>(
        r#"
        select id, slug, title, visibility, markdown_policy, created_by, created_at
        from documents
        where id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    Ok(record)
}

pub async fn delete_by_id(pool: &PgPool, id: uuid::Uuid) -> anyhow::Result<bool> {
    let result = sqlx::query(
        r#"
        delete from documents
        where id = $1
        "#,
    )
    .bind(id)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}
