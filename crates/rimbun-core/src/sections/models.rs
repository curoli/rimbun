use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::{DocumentId, SectionId};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Section {
    pub id: SectionId,
    pub document_id: DocumentId,
    pub parent_id: Option<SectionId>,
    pub title: String,
    pub position: i32,
    pub path: String,
    pub created_at: DateTime<Utc>,
}
