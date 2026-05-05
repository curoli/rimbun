mod config;
mod queue;
mod workers;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let _config = config::Config::from_env()?;
    tracing::info!("rimbun-jobs starting");
    Ok(())
}
