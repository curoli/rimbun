use std::net::SocketAddr;

use rimbun_api::{app, config::Config};
use tokio::net::TcpListener;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();

    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = Config::from_env()?;
    let app = app::build(config.clone()).await?;

    let addr = SocketAddr::from(([127, 0, 0, 1], config.port));
    let listener = TcpListener::bind(addr).await?;

    let worker_pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&config.database_url)
        .await?;
    let worker = tokio::spawn(rimbun_api::db::projection_jobs::run(
        worker_pool,
        rimbun_embedding_client::EmbeddingClient::new(config.embedding_service_url),
    ));

    let result = axum::serve(listener, app).await;
    worker.abort();
    result.map_err(Into::into)
}
