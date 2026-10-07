pub mod types;

use std::time::Duration;

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
        Self::with_timeout(base_url, Duration::from_secs(5))
    }

    pub fn with_timeout(base_url: impl Into<String>, timeout: Duration) -> Self {
        Self {
            base_url: base_url.into(),
            http: Client::builder()
                .timeout(timeout)
                .build()
                .expect("embedding HTTP client configuration is valid"),
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

#[cfg(test)]
mod tests {
    use super::{EmbeddingClient, types::EmbeddingRequest};
    use std::time::Duration;

    #[tokio::test]
    async fn stalled_request_respects_client_timeout() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind stalled server");
        let address = listener.local_addr().expect("server address");
        let server = tokio::spawn(async move {
            let (_stream, _) = listener.accept().await.expect("accept request");
            std::future::pending::<()>().await;
        });
        let client =
            EmbeddingClient::with_timeout(format!("http://{address}"), Duration::from_millis(100));

        let result = tokio::time::timeout(
            Duration::from_secs(1),
            client.embed(&EmbeddingRequest {
                text: "example".to_owned(),
                model_name: None,
            }),
        )
        .await
        .expect("client-level timeout must be bounded");

        assert!(result.is_err());
        server.abort();
    }
}
