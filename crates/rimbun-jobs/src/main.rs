mod config;
mod queue;
mod workers;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let config = config::Config::from_env()?;
    tracing::info!(
        database_url = %config.database_url,
        embedding_service_url = %config.embedding_service_url,
        "rimbun-jobs starting",
    );

    let startup_jobs = vec![
        queue::Job::ComputeEmbedding {
            submission_id: uuid::Uuid::nil(),
        },
        queue::Job::RecomputeProjection {
            section_id: uuid::Uuid::nil(),
        },
    ];

    for job in startup_jobs {
        match job {
            queue::Job::ComputeEmbedding { submission_id } => {
                workers::compute_embedding::run(submission_id).await?;
            }
            queue::Job::RecomputeProjection { section_id } => {
                workers::recompute_projection::run(section_id).await?;
            }
        }
    }

    Ok(())
}
