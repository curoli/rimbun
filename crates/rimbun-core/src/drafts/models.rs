use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::{DraftId, SectionId, SubmissionId, UserId};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Draft {
    pub id: DraftId,
    pub section_id: SectionId,
    pub user_id: UserId,
    pub base_submission_id: Option<SubmissionId>,
    pub markdown_content: String,
    pub updated_at: DateTime<Utc>,
}
