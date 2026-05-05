use serde::{Deserialize, Serialize};

use crate::ids::{SectionId, SubmissionId};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionRole {
    Main,
    PrincipalAlternative,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectionItem {
    pub section_id: SectionId,
    pub submission_id: SubmissionId,
    pub role: ProjectionRole,
    pub rank: i32,
    pub cluster_id: Option<String>,
    pub score: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SectionProjection {
    pub section_id: SectionId,
    pub items: Vec<ProjectionItem>,
}
