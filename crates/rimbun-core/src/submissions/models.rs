use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::{SectionId, SubmissionId, UserId};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SubmissionStatus {
    Published,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Submission {
    pub id: SubmissionId,
    pub section_id: SectionId,
    pub user_id: UserId,
    pub base_submission_id: Option<SubmissionId>,
    pub markdown_content: String,
    pub status: SubmissionStatus,
    pub published_at: DateTime<Utc>,
    pub superseded_by: Option<SubmissionId>,
}
