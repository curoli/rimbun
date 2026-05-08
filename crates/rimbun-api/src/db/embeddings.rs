use std::collections::HashMap;

use rimbun_embedding_client::types::EmbeddingResponse;
use serde::Serialize;
use sqlx::{types::Json, FromRow, PgPool};

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct SubmissionEmbeddingRecord {
    pub submission_id: uuid::Uuid,
    pub model_name: String,
    pub embedding: Json<Vec<f32>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub async fn list_by_submission_ids(
    pool: &PgPool,
    submission_ids: &[uuid::Uuid],
) -> anyhow::Result<HashMap<uuid::Uuid, SubmissionEmbeddingRecord>> {
    if submission_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let records = sqlx::query_as::<_, SubmissionEmbeddingRecord>(
        r#"
        select submission_id, model_name, embedding, created_at
        from submission_embeddings
        where submission_id = any($1)
        "#,
    )
    .bind(submission_ids)
    .fetch_all(pool)
    .await?;

    Ok(records
        .into_iter()
        .map(|record| (record.submission_id, record))
        .collect())
}

pub async fn upsert(
    pool: &PgPool,
    submission_id: uuid::Uuid,
    response: &EmbeddingResponse,
) -> anyhow::Result<SubmissionEmbeddingRecord> {
    let record = sqlx::query_as::<_, SubmissionEmbeddingRecord>(
        r#"
        insert into submission_embeddings (submission_id, model_name, embedding)
        values ($1, $2, $3)
        on conflict (submission_id)
        do update set
          model_name = excluded.model_name,
          embedding = excluded.embedding,
          created_at = now()
        returning submission_id, model_name, embedding, created_at
        "#,
    )
    .bind(submission_id)
    .bind(&response.model_name)
    .bind(Json(response.embedding.clone()))
    .fetch_one(pool)
    .await?;

    Ok(record)
}
