#[derive(Debug, Clone)]
pub enum Job {
    ComputeEmbedding { submission_id: uuid::Uuid },
    RecomputeProjection { section_id: uuid::Uuid },
}
