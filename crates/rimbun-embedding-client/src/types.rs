use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingRequest {
    pub text: String,
    pub model_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingResponse {
    pub model_name: String,
    pub embedding: Vec<f32>,
}
