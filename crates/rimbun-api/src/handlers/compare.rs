use std::collections::HashMap;

use axum::{
    extract::{Path, State},
    http::HeaderMap,
    Json,
};
use chrono::{DateTime, Utc};
use markalign::{
    compare_many, normalize_document, BlockAnchor, BlockKind, ChangeWeight, Comparison,
    ComparisonSet, Document, NormalizedDocument, Options, SourceSpan,
};
use serde::{Deserialize, Serialize};

use crate::{
    db::{documents, projections, sections, submissions},
    error::ApiError,
    http::extractors::maybe_current_user,
    state::AppState,
};

/// Compare payload for one section, intended for `GET /api/sections/:id/compare`.
///
/// This DTO deliberately sits above `markalign`'s raw output types so that
/// Rimbun can stabilize its API independently from library-internal changes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SectionCompareDto {
    pub section_id: uuid::Uuid,
    pub section_title: String,
    pub section_number: String,
    pub main_submission: SubmissionSummaryDto,
    pub alternatives: Vec<SubmissionSummaryDto>,
    pub blocks: Vec<CompareBlockDto>,
}

/// Lightweight submission summary used across compare responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmissionSummaryDto {
    pub submission_id: uuid::Uuid,
    pub user_id: uuid::Uuid,
    pub username: String,
    pub display_name: String,
    pub published_at: DateTime<Utc>,
    pub rank: usize,
    pub support_percent: Option<f64>,
}

/// One reference block in the main version together with per-alternative variants.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompareBlockDto {
    pub block_index: usize,
    pub block_kind: String,
    pub anchor: BlockAnchorDto,
    pub main_text: String,
    pub variants: Vec<BlockVariantDto>,
}

/// Stable structural address for a compared Markdown block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockAnchorDto {
    pub block_path: Vec<usize>,
    pub heading_path: Vec<usize>,
    pub stable_block_path: Vec<String>,
    pub stable_heading_path: Vec<String>,
    pub block_key: String,
    pub list_item_index: Option<usize>,
}

/// One alternative rendering of a reference block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockVariantDto {
    pub alternative_submission_id: uuid::Uuid,
    pub alternative_index: usize,
    pub kind: BlockVariantKindDto,
    pub weight: Option<String>,
    pub text: String,
    pub source_span: Option<SourceSpanDto>,
}

/// Whether an alternative leaves the reference block unchanged or replaces it.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BlockVariantKindDto {
    Unchanged,
    Changed,
}

/// Source span in line/column form for UI highlighting.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceSpanDto {
    pub start_line: usize,
    pub start_column: usize,
    pub end_line: usize,
    pub end_column: usize,
}

pub async fn section_compare(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(section_id): Path<uuid::Uuid>,
) -> Result<Json<SectionCompareDto>, ApiError> {
    let current_user = maybe_current_user(State(state.clone()), &headers).await?;
    let section = sections::find_by_id(&state.pool, section_id)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?
        .ok_or_else(|| ApiError::not_found("section not found"))?;

    let document = documents::find_by_id(&state.pool, section.document_id)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?
        .ok_or_else(|| ApiError::not_found("document not found"))?;

    if document.visibility == "authenticated" && current_user.is_none() {
        return Err(ApiError::unauthorized("authentication required"));
    }

    let projection = projections::list_by_section(&state.pool, section_id)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;
    let active_submissions = submissions::list_active_visible_by_section(&state.pool, section_id)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;

    let projected = projected_submissions(&projection, &active_submissions);
    let main = projected
        .iter()
        .find(|entry| entry.role == "main")
        .cloned()
        .or_else(|| projected.first().cloned())
        .ok_or_else(|| ApiError::bad_request("no published main submission for this section"))?;
    let alternatives = projected
        .iter()
        .filter(|entry| entry.submission.id != main.submission.id)
        .cloned()
        .collect::<Vec<_>>();

    let section_number = compute_section_number(&state, section.document_id, section.id).await?;
    let options = Options::default();
    let main_document =
        Document::with_id(main.submission.id.to_string(), main.submission.markdown_content.clone());
    let alternative_documents = alternatives
        .iter()
        .map(|entry| {
            Document::with_id(
                entry.submission.id.to_string(),
                entry.submission.markdown_content.clone(),
            )
        })
        .collect::<Vec<_>>();
    let alternative_normalized = alternative_documents
        .iter()
        .map(|document| {
            normalize_document(document, &options)
                .map(|normalized| (document.id.clone().unwrap_or_default(), normalized))
        })
        .collect::<Result<HashMap<_, _>, _>>()
        .map_err(|err| ApiError::internal(format!("markalign normalize failed: {err:?}")))?;

    let comparison_set = compare_many(
        &main_document,
        &alternative_documents,
        &options,
    )
    .map_err(|err| ApiError::internal(format!("markalign compare failed: {err:?}")))?;

    let blocks = map_compare_blocks(&comparison_set, &alternatives, &alternative_normalized);

    Ok(Json(SectionCompareDto {
        section_id: section.id,
        section_title: section.title,
        section_number,
        main_submission: map_submission_summary(&main),
        alternatives: alternatives.iter().map(map_submission_summary).collect(),
        blocks,
    }))
}

#[derive(Debug, Clone)]
struct ProjectedSubmission<'a> {
    submission: &'a submissions::SubmissionRecord,
    role: &'a str,
    rank: usize,
    support_percent: Option<f64>,
}

fn projected_submissions<'a>(
    projection: &'a [projections::ProjectionItemRecord],
    active_submissions: &'a [submissions::SubmissionRecord],
) -> Vec<ProjectedSubmission<'a>> {
    let by_id = active_submissions
        .iter()
        .map(|submission| (submission.id, submission))
        .collect::<HashMap<_, _>>();

    let mut projected = projection
        .iter()
        .filter_map(|item| {
            let submission = by_id.get(&item.submission_id)?;
            Some(ProjectedSubmission {
                submission,
                role: item.role.as_str(),
                rank: item.rank as usize,
                support_percent: item.score,
            })
        })
        .collect::<Vec<_>>();
    projected.sort_by_key(|entry| entry.rank);
    projected
}

fn map_submission_summary(entry: &ProjectedSubmission<'_>) -> SubmissionSummaryDto {
    SubmissionSummaryDto {
        submission_id: entry.submission.id,
        user_id: entry.submission.user_id,
        username: entry.submission.username.clone(),
        display_name: entry.submission.display_name.clone(),
        published_at: entry.submission.published_at,
        rank: entry.rank + 1,
        support_percent: entry.support_percent,
    }
}

fn map_compare_blocks(
    comparison_set: &ComparisonSet,
    alternatives: &[ProjectedSubmission<'_>],
    alternative_normalized: &HashMap<String, NormalizedDocument>,
) -> Vec<CompareBlockDto> {
    let alternative_lookup = alternatives
        .iter()
        .enumerate()
        .map(|(index, entry)| (entry.submission.id.to_string(), (index, entry)))
        .collect::<HashMap<_, _>>();
    let comparison_lookup = comparison_set
        .comparisons
        .iter()
        .filter_map(|comparison| {
            comparison
                .alternative_id
                .as_ref()
                .map(|alternative_id| (alternative_id.clone(), comparison))
        })
        .collect::<HashMap<_, _>>();
    let mut variants_by_block =
        HashMap::<usize, HashMap<uuid::Uuid, BlockVariantDto>>::new();

    for region in &comparison_set.shared_unchanged_regions {
        for &block_index in &region.block_indices {
            for (alternative_id, (alternative_index, alternative)) in &alternative_lookup {
                let variant = unchanged_variant_for_block(
                    block_index,
                    alternative_index + 1,
                    alternative.submission.id,
                    comparison_lookup.get(alternative_id).copied(),
                    alternative_normalized.get(alternative_id),
                    &comparison_set.reference_blocks,
                );
                variants_by_block
                    .entry(block_index)
                    .or_default()
                    .insert(alternative.submission.id, variant);
            }
        }
    }

    for cluster in &comparison_set.variant_clusters {
        for &block_index in &cluster.block_indices {
            for variant in &cluster.variants {
                let Some((alternative_index, alternative)) =
                    alternative_lookup.get(&variant.alternative_id)
                else {
                    continue;
                };
                let source_span = alternative_normalized
                    .get(&variant.alternative_id)
                    .map(|normalized| source_span_dto(normalized.source_map().span_for_range(
                        variant.alternative_source_range.clone(),
                    )));
                let text = alternative_normalized
                    .get(&variant.alternative_id)
                    .and_then(|normalized| {
                        normalized
                            .source
                            .get(variant.alternative_source_range.clone())
                            .map(ToOwned::to_owned)
                    })
                    .unwrap_or_else(|| tokens_to_text(&variant.replacement));

                variants_by_block
                    .entry(block_index)
                    .or_default()
                    .insert(
                        alternative.submission.id,
                        BlockVariantDto {
                            alternative_submission_id: alternative.submission.id,
                            alternative_index: *alternative_index + 1,
                            kind: BlockVariantKindDto::Changed,
                            weight: Some(change_weight_label(variant.weight)),
                            text,
                            source_span: source_span,
                        },
                    );
            }

            for alternative_id in &cluster.unchanged_alternatives {
                let Some((alternative_index, alternative)) =
                    alternative_lookup.get(alternative_id)
                else {
                    continue;
                };
                let variant = unchanged_variant_for_block(
                    block_index,
                    alternative_index + 1,
                    alternative.submission.id,
                    comparison_lookup.get(alternative_id).copied(),
                    alternative_normalized.get(alternative_id),
                    &comparison_set.reference_blocks,
                );
                variants_by_block
                    .entry(block_index)
                    .or_default()
                    .insert(alternative.submission.id, variant);
            }
        }
    }

    comparison_set
        .reference_blocks
        .iter()
        .map(|block| {
            let mut variants = variants_by_block
                .remove(&block.index)
                .unwrap_or_default()
                .into_values()
                .collect::<Vec<_>>();
            variants.sort_by_key(|variant| variant.alternative_index);
            CompareBlockDto {
                block_index: block.index,
                block_kind: block_kind_label(&block.kind),
                anchor: map_block_anchor(&block.anchor),
                main_text: block.text.clone(),
                variants,
            }
        })
        .collect()
}

fn map_block_anchor(anchor: &BlockAnchor) -> BlockAnchorDto {
    BlockAnchorDto {
        block_path: anchor.block_path.clone(),
        heading_path: anchor.heading_path.clone(),
        stable_block_path: anchor.stable_block_path.clone(),
        stable_heading_path: anchor.stable_heading_path.clone(),
        block_key: anchor.block_key.clone(),
        list_item_index: anchor.list_item_index,
    }
}

fn change_weight_label(weight: ChangeWeight) -> String {
    match weight {
        ChangeWeight::Small => "small".to_owned(),
        ChangeWeight::Medium => "medium".to_owned(),
        ChangeWeight::Large => "large".to_owned(),
    }
}

fn block_kind_label(kind: &BlockKind) -> String {
    match kind {
        BlockKind::Paragraph => "paragraph",
        BlockKind::Heading => "heading",
        BlockKind::BlockQuote => "blockquote",
        BlockKind::ListItem => "list_item",
        BlockKind::CodeBlock => "code_block",
        BlockKind::TableRow => "table_row",
        BlockKind::HtmlBlock => "html_block",
        BlockKind::FootnoteDefinition => "footnote_definition",
    }
    .to_owned()
}

fn source_span_dto(span: SourceSpan) -> SourceSpanDto {
    SourceSpanDto {
        start_line: span.start.line,
        start_column: span.start.column,
        end_line: span.end.line,
        end_column: span.end.column,
    }
}

fn unchanged_variant_for_block(
    block_index: usize,
    alternative_index: usize,
    alternative_submission_id: uuid::Uuid,
    comparison: Option<&Comparison>,
    alternative_normalized: Option<&NormalizedDocument>,
    reference_blocks: &[markalign::ReferenceBlock],
) -> BlockVariantDto {
    let reference_block = reference_blocks
        .iter()
        .find(|block| block.index == block_index);
    let alternative_block = comparison
        .and_then(|comparison| {
            reference_block.and_then(|reference_block| {
                comparison.alternative_blocks.iter().find(|block| {
                    block.anchor.stable_block_path == reference_block.anchor.stable_block_path
                })
            })
        });
    let source_span = alternative_block.and_then(|block| {
        alternative_normalized.map(|normalized| {
            source_span_dto(normalized.source_map().span_for_range(block.source_range.clone()))
        })
    });
    let text = alternative_block
        .and_then(|block| {
            alternative_normalized.and_then(|normalized| {
                normalized
                    .source
                    .get(block.source_range.clone())
                    .map(ToOwned::to_owned)
            })
        })
        .or_else(|| alternative_block.map(|block| block.text.clone()))
        .or_else(|| reference_block.map(|block| block.text.clone()))
        .unwrap_or_default();

    BlockVariantDto {
        alternative_submission_id,
        alternative_index,
        kind: BlockVariantKindDto::Unchanged,
        weight: None,
        text,
        source_span,
    }
}

fn tokens_to_text(tokens: &[markalign::Token]) -> String {
    tokens
        .iter()
        .filter_map(|token| match token {
            markalign::Token::Text(text) => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}

async fn compute_section_number(
    state: &AppState,
    document_id: uuid::Uuid,
    section_id: uuid::Uuid,
) -> Result<String, ApiError> {
    let sections = sections::list_by_document(&state.pool, document_id)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;

    let mut by_parent = HashMap::<Option<uuid::Uuid>, Vec<sections::SectionRecord>>::new();
    for section in sections {
        by_parent.entry(section.parent_id).or_default().push(section);
    }

    for group in by_parent.values_mut() {
        group.sort_by(|left, right| {
            left.position
                .cmp(&right.position)
                .then(left.created_at.cmp(&right.created_at))
        });
    }

    fn visit(
        by_parent: &HashMap<Option<uuid::Uuid>, Vec<sections::SectionRecord>>,
        parent_id: Option<uuid::Uuid>,
        prefix: &[usize],
        target_section_id: uuid::Uuid,
    ) -> Option<String> {
        let children = by_parent.get(&parent_id)?;
        for (index, child) in children.iter().enumerate() {
            let next_prefix = {
                let mut next = prefix.to_vec();
                next.push(index + 1);
                next
            };
            if child.id == target_section_id {
                return Some(
                    next_prefix
                        .iter()
                        .map(|part| part.to_string())
                        .collect::<Vec<_>>()
                        .join("."),
                );
            }
            if let Some(found) = visit(by_parent, Some(child.id), &next_prefix, target_section_id) {
                return Some(found);
            }
        }
        None
    }

    visit(&by_parent, None, &[], section_id)
        .ok_or_else(|| ApiError::internal("failed to compute section number"))
}
