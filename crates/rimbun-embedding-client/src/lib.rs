pub mod types;

use reqwest::Client;
use thiserror::Error;
use types::{EmbeddingRequest, EmbeddingResponse};

#[derive(Debug, Error)]
pub enum EmbeddingClientError {
    #[error("request failed: {0}")]
    Request(#[from] reqwest::Error),
}

#[derive(Clone)]
pub struct EmbeddingClient {
    base_url: String,
    http: Client,
}

impl EmbeddingClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            http: Client::new(),
        }
    }

    pub async fn embed(
        &self,
        request: &EmbeddingRequest,
    ) -> Result<EmbeddingResponse, EmbeddingClientError> {
        let response = self
            .http
            .post(format!("{}/embed", self.base_url))
            .json(request)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        Ok(response)
    }
}
