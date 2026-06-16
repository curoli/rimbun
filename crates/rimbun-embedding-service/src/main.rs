use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use axum::{
    Json, Router,
    extract::State,
    routing::{get, post},
};
use popsam_core::{CandleEmbeddingProvider, EmbeddingProvider, InputRecord};
use rimbun_embedding_client::types::{EmbeddingRequest, EmbeddingResponse};
use serde::Serialize;
use tower_http::trace::TraceLayer;

#[derive(Clone)]
struct AppState {
    provider: Arc<Mutex<CandleEmbeddingProvider>>,
    model_name: String,
}

#[derive(Clone, Debug)]
struct Config {
    port: u16,
    model_name: String,
}

impl Config {
    fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            port: std::env::var("RIMBUN_EMBEDDING_PORT")
                .unwrap_or_else(|_| "8001".to_owned())
                .parse()?,
            model_name: std::env::var("RIMBUN_EMBEDDING_MODEL").unwrap_or_else(|_| {
                "sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2".to_owned()
            }),
        })
    }
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
    model_name: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();

    tracing_subscriber::fmt::init();

    let config = Config::from_env()?;
    tracing::info!("loading local embedding model {}", config.model_name);

    let provider = tokio::task::spawn_blocking(|| CandleEmbeddingProvider::cpu(true))
        .await
        .map_err(|err| anyhow::anyhow!("embedding provider task failed: {err}"))??;

    let state = AppState {
        provider: Arc::new(Mutex::new(provider)),
        model_name: config.model_name.clone(),
    };

    let app = Router::new()
        .route("/health", get(health))
        .route("/embed", post(embed))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], config.port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("embedding service listening on {}", addr);
    axum::serve(listener, app).await?;
    Ok(())
}

async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        model_name: state.model_name,
    })
}

async fn embed(
    State(state): State<AppState>,
    Json(request): Json<EmbeddingRequest>,
) -> Result<Json<EmbeddingResponse>, (axum::http::StatusCode, String)> {
    let model_name = request
        .model_name
        .clone()
        .unwrap_or_else(|| state.model_name.clone());
    let provider = Arc::clone(&state.provider);
    let text = request.text;

    let embedded = tokio::task::spawn_blocking(move || {
        let provider = provider
            .lock()
            .map_err(|_| anyhow::anyhow!("embedding provider mutex poisoned"))?;
        let records = vec![InputRecord {
            id: "request-0".to_owned(),
            text: Some(text),
        }];
        let mut embedded = provider.embed(&records)?;
        embedded
            .pop()
            .ok_or_else(|| anyhow::anyhow!("embedding provider returned no vector"))
    })
    .await
    .map_err(internal_error)?
    .map_err(internal_error)?;

    Ok(Json(EmbeddingResponse {
        model_name,
        embedding: embedded.embedding,
    }))
}

fn internal_error<E: std::fmt::Display>(error: E) -> (axum::http::StatusCode, String) {
    (
        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        error.to_string(),
    )
}
