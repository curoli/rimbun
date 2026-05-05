use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::{DocumentId, UserId};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    Public,
    Authenticated,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MarkdownPolicy {
    pub allow_links: bool,
    pub allow_code_blocks: bool,
    pub allow_blockquotes: bool,
    pub allow_lists: bool,
    pub allow_raw_html: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub id: DocumentId,
    pub slug: String,
    pub title: String,
    pub visibility: Visibility,
    pub markdown_policy: MarkdownPolicy,
    pub created_by: UserId,
    pub created_at: DateTime<Utc>,
}
