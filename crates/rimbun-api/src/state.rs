use sqlx::PgPool;

use crate::config::Config;
use rimbun_embedding_client::EmbeddingClient;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: Config,
    pub embedding_client: EmbeddingClient,
}

impl AppState {
    pub fn new(pool: PgPool, config: Config) -> Self {
        let embedding_client = EmbeddingClient::new(config.embedding_service_url.clone());
        Self {
            pool,
            config,
            embedding_client,
        }
    }
}
