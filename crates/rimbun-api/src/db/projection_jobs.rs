use rimbun_embedding_client::EmbeddingClient;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

#[derive(Debug, FromRow)]
pub struct ClaimedJob {
    pub section_id: Uuid,
    pub revision: i64,
    pub attempts: i32,
    pub lease_token: Uuid,
}

pub async fn claim(pool: &PgPool) -> anyhow::Result<Option<ClaimedJob>> {
    Ok(sqlx::query_as::<_, ClaimedJob>(
        r#"
        with candidate as (
            select section_id from projection_jobs
            where available_at <= now()
              and (lease_until is null or lease_until <= now())
            order by available_at, section_id
            for update skip locked limit 1
        )
        update projection_jobs j
        set lease_until = now() + interval '60 seconds', lease_token = $1
        from candidate c where j.section_id = c.section_id
        returning j.section_id, j.revision, j.attempts, j.lease_token
        "#,
    )
    .bind(Uuid::new_v4())
    .fetch_optional(pool)
    .await?)
}

pub async fn finish(pool: &PgPool, job: &ClaimedJob, error: Option<&str>) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;
    if error.is_none() {
        sqlx::query(
            "delete from projection_jobs where section_id = $1 and revision = $2 and lease_token = $3",
        )
        .bind(job.section_id)
        .bind(job.revision)
        .bind(job.lease_token)
        .execute(&mut *tx)
        .await?;
    }
    let delay = 2_i32.pow(job.attempts.saturating_add(1).clamp(1, 8) as u32);
    sqlx::query(
        r#"
        update projection_jobs set
            lease_until = null, lease_token = null,
            attempts = case when revision = $2 and $4::text is not null then attempts + 1 else attempts end,
            last_error = case when revision = $2 then $4 else last_error end,
            available_at = case when revision = $2 and $4::text is not null
                then now() + $5 * interval '1 second' else available_at end
        where section_id = $1 and lease_token = $3
        "#,
    )
    .bind(job.section_id)
    .bind(job.revision)
    .bind(job.lease_token)
    .bind(error)
    .bind(delay)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn run(pool: PgPool, client: EmbeddingClient) {
    loop {
        match process_next(&pool, &client).await {
            Ok(true) => continue,
            Ok(false) => {}
            Err(error) => tracing::error!(%error, "projection worker failed"),
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
}

pub async fn process_next(pool: &PgPool, client: &EmbeddingClient) -> anyhow::Result<bool> {
    let Some(job) = claim(pool).await? else {
        return Ok(false);
    };
    let result =
        super::projections::rebuild_trivial_for_section(pool, client, job.section_id).await;
    let error = result.err().map(|error| format!("{error:#}"));
    if let Some(error) = &error {
        tracing::warn!(section_id = %job.section_id, %error, "projection rebuild queued for retry");
    }
    finish(pool, &job, error.as_deref()).await?;
    Ok(true)
}
