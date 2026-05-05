use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::{SubmissionId, UserId};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmissionModeration {
    pub submission_id: SubmissionId,
    pub hidden: bool,
    pub soft_deleted: bool,
    pub excluded_from_clustering: bool,
    pub reason: Option<String>,
    pub moderated_by: Option<UserId>,
    pub moderated_at: Option<DateTime<Utc>>,
}
