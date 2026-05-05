use serde::Serialize;
use sqlx::{FromRow, PgPool};

use crate::db::submissions::SubmissionRecord;

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct ProjectionItemRecord {
    pub section_id: uuid::Uuid,
    pub submission_id: uuid::Uuid,
    pub role: String,
    pub rank: i32,
    pub cluster_id: Option<String>,
    pub score: Option<f64>,
}

pub async fn list_by_section(
    pool: &PgPool,
    section_id: uuid::Uuid,
) -> anyhow::Result<Vec<ProjectionItemRecord>> {
    let records = sqlx::query_as::<_, ProjectionItemRecord>(
        r#"
        select section_id, submission_id, role, rank, cluster_id, score
        from section_projection_items
        where section_id = $1
        order by rank asc
        "#,
    )
    .bind(section_id)
    .fetch_all(pool)
    .await?;

    Ok(records)
}

pub async fn replace_for_section(
    pool: &PgPool,
    section_id: uuid::Uuid,
    submissions: &[SubmissionRecord],
) -> anyhow::Result<Vec<ProjectionItemRecord>> {
    sqlx::query(
        r#"
        delete from section_projection_items
        where section_id = $1
        "#,
    )
    .bind(section_id)
    .execute(pool)
    .await?;

    let mut items = Vec::with_capacity(submissions.len());

    for (index, submission) in submissions.iter().enumerate() {
        let role = if index == 0 {
            "main"
        } else if index <= 4 {
            "principal_alternative"
        } else {
            "other"
        };

        let record = sqlx::query_as::<_, ProjectionItemRecord>(
            r#"
            insert into section_projection_items (
              section_id,
              submission_id,
              role,
              rank,
              cluster_id,
              score
            )
            values ($1, $2, $3, $4, $5, $6)
            returning section_id, submission_id, role, rank, cluster_id, score
            "#,
        )
        .bind(section_id)
        .bind(submission.id)
        .bind(role)
        .bind(index as i32)
        .bind(Option::<String>::None)
        .bind(Option::<f64>::None)
        .fetch_one(pool)
        .await?;

        items.push(record);
    }

    Ok(items)
}

pub async fn rebuild_trivial_for_section(
    pool: &PgPool,
    section_id: uuid::Uuid,
) -> anyhow::Result<Vec<ProjectionItemRecord>> {
    let active_submissions =
        crate::db::submissions::list_active_clusterable_visible_by_section(pool, section_id).await?;
    replace_for_section(pool, section_id, &active_submissions).await
}
