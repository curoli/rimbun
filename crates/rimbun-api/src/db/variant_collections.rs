use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{FromRow, PgPool};

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct VariantCollectionRecord {
    pub id: uuid::Uuid,
    pub name: String,
    pub description: String,
    pub created_by: uuid::Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewVariantCollection {
    pub id: uuid::Uuid,
    pub name: String,
    pub description: String,
    pub created_by: uuid::Uuid,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct VariantEntryRecord {
    pub id: uuid::Uuid,
    pub collection_id: uuid::Uuid,
    pub position: i32,
    pub label: String,
    pub username_hint: Option<String>,
    pub markdown_content: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewVariantEntry {
    pub id: uuid::Uuid,
    pub collection_id: uuid::Uuid,
    pub position: i32,
    pub label: String,
    pub username_hint: Option<String>,
    pub markdown_content: String,
}

pub async fn list_collections(pool: &PgPool) -> anyhow::Result<Vec<VariantCollectionRecord>> {
    let records = sqlx::query_as::<_, VariantCollectionRecord>(
        r#"
        select id, name, description, created_by, created_at, updated_at
        from variant_collections
        order by created_at asc, name asc
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(records)
}

pub async fn find_collection_by_id(
    pool: &PgPool,
    id: uuid::Uuid,
) -> anyhow::Result<Option<VariantCollectionRecord>> {
    let record = sqlx::query_as::<_, VariantCollectionRecord>(
        r#"
        select id, name, description, created_by, created_at, updated_at
        from variant_collections
        where id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    Ok(record)
}

pub async fn create_collection(
    pool: &PgPool,
    collection: &NewVariantCollection,
) -> anyhow::Result<VariantCollectionRecord> {
    let record = sqlx::query_as::<_, VariantCollectionRecord>(
        r#"
        insert into variant_collections (id, name, description, created_by)
        values ($1, $2, $3, $4)
        returning id, name, description, created_by, created_at, updated_at
        "#,
    )
    .bind(collection.id)
    .bind(&collection.name)
    .bind(&collection.description)
    .bind(collection.created_by)
    .fetch_one(pool)
    .await?;

    Ok(record)
}

pub async fn update_collection(
    pool: &PgPool,
    id: uuid::Uuid,
    name: &str,
    description: &str,
) -> anyhow::Result<Option<VariantCollectionRecord>> {
    let record = sqlx::query_as::<_, VariantCollectionRecord>(
        r#"
        update variant_collections
        set name = $2, description = $3, updated_at = now()
        where id = $1
        returning id, name, description, created_by, created_at, updated_at
        "#,
    )
    .bind(id)
    .bind(name)
    .bind(description)
    .fetch_optional(pool)
    .await?;

    Ok(record)
}

pub async fn delete_collection(pool: &PgPool, id: uuid::Uuid) -> anyhow::Result<bool> {
    let result = sqlx::query(
        r#"
        delete from variant_collections
        where id = $1
        "#,
    )
    .bind(id)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

pub async fn list_entries(pool: &PgPool, collection_id: uuid::Uuid) -> anyhow::Result<Vec<VariantEntryRecord>> {
    let records = sqlx::query_as::<_, VariantEntryRecord>(
        r#"
        select id, collection_id, position, label, username_hint, markdown_content, created_at, updated_at
        from variant_entries
        where collection_id = $1
        order by position asc, created_at asc
        "#,
    )
    .bind(collection_id)
    .fetch_all(pool)
    .await?;

    Ok(records)
}

pub async fn find_entry_by_id(
    pool: &PgPool,
    id: uuid::Uuid,
) -> anyhow::Result<Option<VariantEntryRecord>> {
    let record = sqlx::query_as::<_, VariantEntryRecord>(
        r#"
        select id, collection_id, position, label, username_hint, markdown_content, created_at, updated_at
        from variant_entries
        where id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    Ok(record)
}

pub async fn create_entry(pool: &PgPool, entry: &NewVariantEntry) -> anyhow::Result<VariantEntryRecord> {
    let record = sqlx::query_as::<_, VariantEntryRecord>(
        r#"
        insert into variant_entries (id, collection_id, position, label, username_hint, markdown_content)
        values ($1, $2, $3, $4, $5, $6)
        returning id, collection_id, position, label, username_hint, markdown_content, created_at, updated_at
        "#,
    )
    .bind(entry.id)
    .bind(entry.collection_id)
    .bind(entry.position)
    .bind(&entry.label)
    .bind(&entry.username_hint)
    .bind(&entry.markdown_content)
    .fetch_one(pool)
    .await?;

    Ok(record)
}

pub async fn update_entry(
    pool: &PgPool,
    id: uuid::Uuid,
    position: i32,
    label: &str,
    username_hint: Option<&str>,
    markdown_content: &str,
) -> anyhow::Result<Option<VariantEntryRecord>> {
    let record = sqlx::query_as::<_, VariantEntryRecord>(
        r#"
        update variant_entries
        set
          position = $2,
          label = $3,
          username_hint = $4,
          markdown_content = $5,
          updated_at = now()
        where id = $1
        returning id, collection_id, position, label, username_hint, markdown_content, created_at, updated_at
        "#,
    )
    .bind(id)
    .bind(position)
    .bind(label)
    .bind(username_hint)
    .bind(markdown_content)
    .fetch_optional(pool)
    .await?;

    Ok(record)
}

pub async fn delete_entry(pool: &PgPool, id: uuid::Uuid) -> anyhow::Result<bool> {
    let result = sqlx::query(
        r#"
        delete from variant_entries
        where id = $1
        "#,
    )
    .bind(id)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}
