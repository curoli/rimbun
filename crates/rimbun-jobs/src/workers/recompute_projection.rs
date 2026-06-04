pub async fn run(section_id: uuid::Uuid) -> anyhow::Result<()> {
    tracing::info!(%section_id, "recompute_projection worker stub");
    Ok(())
}
