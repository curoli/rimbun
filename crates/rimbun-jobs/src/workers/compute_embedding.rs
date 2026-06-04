pub async fn run(submission_id: uuid::Uuid) -> anyhow::Result<()> {
    tracing::info!(%submission_id, "compute_embedding worker stub");
    Ok(())
}
