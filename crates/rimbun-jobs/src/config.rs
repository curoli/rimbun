#[derive(Clone, Debug)]
pub struct Config {
    pub database_url: String,
    pub embedding_service_url: String,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            database_url: std::env::var("DATABASE_URL")?,
            embedding_service_url: std::env::var("EMBEDDING_SERVICE_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8001".to_owned()),
        })
    }
}
