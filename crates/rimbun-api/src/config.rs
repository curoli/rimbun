#[derive(Clone, Debug)]
pub struct Config {
    pub port: u16,
    pub database_url: String,
    pub session_secret: String,
    pub embedding_service_url: String,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            port: std::env::var("RIMBUN_PORT")
                .unwrap_or_else(|_| "3000".to_owned())
                .parse()?,
            database_url: std::env::var("DATABASE_URL")?,
            session_secret: std::env::var("SESSION_SECRET")?,
            embedding_service_url: std::env::var("EMBEDDING_SERVICE_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8001".to_owned()),
        })
    }
}
