use std::collections::HashMap;

use popsam_core::{
    run_election, CandidateBestResult, ElectionConfig, EmbeddedTextInput, ElectionResult,
};
use rimbun_embedding_client::{types::EmbeddingRequest, EmbeddingClient};
use serde::Serialize;
use sqlx::{FromRow, PgPool};

use crate::db::{embeddings, submissions::SubmissionRecord};

const PROJECTION_REPRESENTATIVE_COUNT: usize = 5;
const LEXICAL_EMBEDDING_DIMENSIONS: usize = 128;

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct ProjectionItemRecord {
    pub section_id: uuid::Uuid,
    pub submission_id: uuid::Uuid,
    pub role: String,
    pub rank: i32,
    pub cluster_id: Option<String>,
    pub score: Option<f64>,
}

#[derive(Debug, Clone)]
struct RankedProjectionItem {
    submission_id: uuid::Uuid,
    role: &'static str,
    rank: i32,
    cluster_id: Option<String>,
    score: Option<f64>,
}

pub async fn list_by_section(
    pool: &PgPool,
    section_id: uuid::Uuid,
) -> anyhow::Result<Vec<ProjectionItemRecord>> {
    let records = sqlx::query_as::<_, ProjectionItemRecord>(
        r#"
        select section_id, submission_id, role, rank, cluster_id, score
        from section_projection_items
        where section_id = $1
        order by rank asc
        "#,
    )
    .bind(section_id)
    .fetch_all(pool)
    .await?;

    Ok(records)
}

async fn replace_for_section(
    pool: &PgPool,
    section_id: uuid::Uuid,
    items: &[RankedProjectionItem],
) -> anyhow::Result<Vec<ProjectionItemRecord>> {
    sqlx::query(
        r#"
        delete from section_projection_items
        where section_id = $1
        "#,
    )
    .bind(section_id)
    .execute(pool)
    .await?;

    let mut records = Vec::with_capacity(items.len());

    for item in items {
        let record = sqlx::query_as::<_, ProjectionItemRecord>(
            r#"
            insert into section_projection_items (
              section_id,
              submission_id,
              role,
              rank,
              cluster_id,
              score
            )
            values ($1, $2, $3, $4, $5, $6)
            returning section_id, submission_id, role, rank, cluster_id, score
            "#,
        )
        .bind(section_id)
        .bind(item.submission_id)
        .bind(item.role)
        .bind(item.rank)
        .bind(&item.cluster_id)
        .bind(item.score)
        .fetch_one(pool)
        .await?;

        records.push(record);
    }

    Ok(records)
}

pub async fn rebuild_trivial_for_section(
    pool: &PgPool,
    embedding_client: &EmbeddingClient,
    section_id: uuid::Uuid,
) -> anyhow::Result<Vec<ProjectionItemRecord>> {
    let active_submissions =
        crate::db::submissions::list_active_clusterable_visible_by_section(pool, section_id).await?;
    let ranked_items = rank_submissions_with_popsam(pool, embedding_client, &active_submissions).await?;
    replace_for_section(pool, section_id, &ranked_items).await
}

async fn rank_submissions_with_popsam(
    pool: &PgPool,
    embedding_client: &EmbeddingClient,
    submissions: &[SubmissionRecord],
) -> anyhow::Result<Vec<RankedProjectionItem>> {
    if submissions.is_empty() {
        return Ok(Vec::new());
    }

    let inputs = build_embedding_inputs(pool, embedding_client, submissions).await?;
    let result = run_election(
        inputs,
        ElectionConfig {
            report_last_k: submissions.len().min(PROJECTION_REPRESENTATIVE_COUNT),
            elimination_fraction: 0.5,
            random_seed: 42,
        },
    )?;

    Ok(build_ranked_items(submissions, &result))
}

async fn build_embedding_inputs(
    pool: &PgPool,
    embedding_client: &EmbeddingClient,
    submissions: &[SubmissionRecord],
) -> anyhow::Result<Vec<EmbeddedTextInput>> {
    let submission_ids = submissions.iter().map(|submission| submission.id).collect::<Vec<_>>();
    let stored_embeddings = embeddings::list_by_submission_ids(pool, &submission_ids).await?;

    let mut inputs = Vec::with_capacity(submissions.len());

    for submission in submissions {
        if let Some(stored) = stored_embeddings.get(&submission.id) {
            inputs.push(EmbeddedTextInput {
                id: submission.id.to_string(),
                text: Some(submission.markdown_content.clone()),
                embedding: stored.embedding.0.clone(),
            });
            continue;
        }

        let embedding = match embedding_client
            .embed(&EmbeddingRequest {
                text: submission.markdown_content.clone(),
                model_name: None,
            })
            .await
        {
            Ok(response) => {
                let stored = embeddings::upsert(pool, submission.id, &response).await?;
                stored.embedding.0
            }
            Err(_) => lexical_embedding(&submission.markdown_content),
        };

        inputs.push(EmbeddedTextInput {
            id: submission.id.to_string(),
            text: Some(submission.markdown_content.clone()),
            embedding,
        });
    }

    Ok(inputs)
}

fn build_ranked_items(
    submissions: &[SubmissionRecord],
    result: &ElectionResult,
) -> Vec<RankedProjectionItem> {
    let submission_ids = submissions
        .iter()
        .map(|submission| (submission.id.to_string(), submission.id))
        .collect::<HashMap<_, _>>();
    let embeddings = result
        .embeddings
        .iter()
        .map(|embedding| (embedding.id.clone(), embedding.embedding.clone()))
        .collect::<HashMap<_, _>>();
    let anchors = cluster_anchor_ids(result);
    let best_results = best_results_by_candidate(result);
    let total_versions = result.embeddings.len() as f64;

    result
        .all_ranked_ids
        .iter()
        .enumerate()
        .filter_map(|(index, submission_id)| {
            let parsed_submission_id = submission_ids.get(submission_id).copied()?;
            let role = if index == 0 {
                "main"
            } else if index <= 4 {
                "principal_alternative"
            } else {
                "other"
            };

            let cluster_id = assign_cluster_id(submission_id, &anchors, &embeddings);
            let score = best_results
                .get(submission_id)
                .map(|result| best_round_support_percentage(result, total_versions));

            Some(RankedProjectionItem {
                submission_id: parsed_submission_id,
                role,
                rank: index as i32,
                cluster_id: cluster_id.or_else(|| Some(submission_id.clone())),
                score,
            })
        })
        .collect()
}

fn cluster_anchor_ids(result: &ElectionResult) -> Vec<String> {
    let anchor_count = match result.all_ranked_ids.len() {
        0 => 0,
        1 | 2 => 1,
        3..=6 => 2,
        _ => 3,
    };

    result
        .all_ranked_ids
        .iter()
        .take(anchor_count)
        .cloned()
        .collect()
}

fn assign_cluster_id(
    submission_id: &str,
    anchors: &[String],
    embeddings: &HashMap<String, Vec<f32>>,
) -> Option<String> {
    let embedding = embeddings.get(submission_id)?;
    anchors
        .iter()
        .filter_map(|anchor_id| {
            let anchor_embedding = embeddings.get(anchor_id)?;
            Some((anchor_id.clone(), cosine_similarity(embedding, anchor_embedding)))
        })
        .max_by(|left, right| left.1.total_cmp(&right.1))
        .map(|(anchor_id, _)| anchor_id)
}

fn cosine_similarity(left: &[f32], right: &[f32]) -> f32 {
    left.iter().zip(right.iter()).map(|(a, b)| a * b).sum()
}

fn best_results_by_candidate(result: &ElectionResult) -> HashMap<String, CandidateBestResult> {
    result
        .candidate_best_results
        .iter()
        .cloned()
        .map(|result| (result.id.clone(), result))
        .collect()
}

fn best_round_support_percentage(result: &CandidateBestResult, total_versions: f64) -> f64 {
    if total_versions <= 0.0 {
        return 0.0;
    }

    f64::from(result.first_votes) / total_versions * 100.0
}

fn lexical_embedding(markdown_content: &str) -> Vec<f32> {
    let mut embedding = vec![0.0_f32; LEXICAL_EMBEDDING_DIMENSIONS];
    let tokens = tokenize(markdown_content);

    if tokens.is_empty() {
        embedding[0] = 1.0;
        return embedding;
    }

    for token in &tokens {
        let token_hash = stable_hash(token.as_bytes());
        embedding[token_hash as usize % LEXICAL_EMBEDDING_DIMENSIONS] += 1.0;
    }

    for window in tokens.windows(2) {
        let mut phrase = String::with_capacity(window[0].len() + window[1].len() + 1);
        phrase.push_str(&window[0]);
        phrase.push(' ');
        phrase.push_str(&window[1]);
        let phrase_hash = stable_hash(phrase.as_bytes());
        embedding[phrase_hash as usize % LEXICAL_EMBEDDING_DIMENSIONS] += 0.75;
    }

    for (index, line) in markdown_content.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let line_hash = stable_hash(format!("{index}:{}", line.trim().to_lowercase()).as_bytes());
        embedding[line_hash as usize % LEXICAL_EMBEDDING_DIMENSIONS] += 0.35;
    }

    embedding
}

fn tokenize(markdown_content: &str) -> Vec<String> {
    markdown_content
        .split(|character: char| !character.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(|token| token.to_lowercase())
        .collect()
}

fn stable_hash(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    fn submission(id: uuid::Uuid, markdown_content: &str) -> SubmissionRecord {
        SubmissionRecord {
            id,
            section_id: uuid::Uuid::new_v4(),
            user_id: uuid::Uuid::new_v4(),
            username: format!("user-{id}"),
            display_name: format!("User {id}"),
            base_submission_id: None,
            markdown_content: markdown_content.to_owned(),
            status: "published".to_owned(),
            published_at: chrono::Utc::now(),
            superseded_by: None,
        }
    }

    #[test]
    fn lexical_embedding_is_stable_for_equivalent_text() {
        let left = lexical_embedding("# Title\nHello semantic world");
        let right = lexical_embedding("# Title\nHello semantic world");
        assert_eq!(left, right);
    }

    #[test]
    fn build_ranked_items_marks_roles_and_clusters() {
        let anchor_a = uuid::Uuid::new_v4();
        let anchor_b = uuid::Uuid::new_v4();
        let outlier = uuid::Uuid::new_v4();

        let result = run_election(
            vec![
                EmbeddedTextInput {
                    id: anchor_a.to_string(),
                    text: None,
                    embedding: vec![1.0, 0.0, 0.0],
                },
                EmbeddedTextInput {
                    id: anchor_b.to_string(),
                    text: None,
                    embedding: vec![0.95, 0.05, 0.0],
                },
                EmbeddedTextInput {
                    id: outlier.to_string(),
                    text: None,
                    embedding: vec![0.0, 0.0, 1.0],
                },
            ],
            ElectionConfig {
                report_last_k: 3,
                elimination_fraction: 0.5,
                random_seed: 42,
            },
        )
        .expect("run election");

        let ranked = build_ranked_items(
            &[
                submission(anchor_a, "cats"),
                submission(anchor_b, "cats again"),
                submission(outlier, "databases"),
            ],
            &result,
        );

        assert_eq!(ranked.len(), 3);
        assert_eq!(ranked[0].role, "main");
        assert_eq!(ranked[1].role, "principal_alternative");
        assert_eq!(ranked[2].role, "principal_alternative");
        assert!(ranked.iter().all(|item| item.cluster_id.is_some()));
        assert!(ranked.iter().all(|item| item.score.is_some()));
    }

    #[test]
    fn best_round_support_uses_first_votes_over_total_versions() {
        let result = CandidateBestResult {
            id: "candidate-a".to_owned(),
            full_round_index: 2,
            active_candidates: 2,
            rank: 1,
            first_votes: 3,
            second_votes: 0,
            third_votes: 0,
        };

        assert_eq!(best_round_support_percentage(&result, 3.0), 100.0);
        assert_eq!(best_round_support_percentage(&result, 6.0), 50.0);
    }
}
