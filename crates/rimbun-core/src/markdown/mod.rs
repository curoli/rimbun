use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MarkdownValidationResult {
    pub valid: bool,
    pub errors: Vec<String>,
}
