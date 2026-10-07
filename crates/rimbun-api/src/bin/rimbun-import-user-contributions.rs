use std::{collections::HashMap, fs, process::ExitCode};

use anyhow::{Context, Result, bail, ensure};
use clap::Parser;
use serde::Deserialize;
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

use rimbun_api::{
    config::Config,
    db::{comments, drafts, projections, sections, submissions, users},
};
use rimbun_embedding_client::EmbeddingClient;

#[derive(Debug, Parser)]
struct Args {
    username: String,
    input_file: String,
    #[arg(long)]
    publish: bool,
    #[arg(long)]
    dry_run: bool,
}

#[derive(Debug, Deserialize)]
struct ImportFile {
    format_version: u32,
    user: ImportUser,
    entries: Vec<ImportEntry>,
}

#[derive(Debug, Deserialize)]
struct ImportUser {
    username: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ImportEntry {
    section_id: Uuid,
    #[serde(default)]
    section_number: Option<String>,
    #[serde(default)]
    section_title: Option<String>,
    #[serde(default)]
    reference_submission_id: Option<Uuid>,
    base_submission_id: Option<Uuid>,
    draft_markdown: String,
    draft_main_comment_markdown: Option<String>,
}

fn entry_label(entry: &ImportEntry) -> String {
    match (&entry.section_number, &entry.section_title) {
        (Some(number), Some(title)) if !title.is_empty() => format!("{number} {title}"),
        (Some(number), _) => number.clone(),
        (_, Some(title)) if !title.is_empty() => title.clone(),
        _ => entry.section_id.to_string(),
    }
}

fn validate_references(
    entries: &[ImportEntry],
    references: &HashMap<Uuid, Uuid>,
    enforce_snapshot: bool,
) -> Result<()> {
    if !enforce_snapshot {
        return Ok(());
    }
    for entry in entries {
        let current_reference = references.get(&entry.section_id).copied();
        ensure!(
            current_reference == entry.reference_submission_id,
            "section '{}' changed since export (expected main submission {}, current {})",
            entry_label(entry),
            entry
                .reference_submission_id
                .map(|id| id.to_string())
                .unwrap_or_else(|| "none".to_owned()),
            current_reference
                .map(|id| id.to_string())
                .unwrap_or_else(|| "none".to_owned())
        );
    }
    Ok(())
}

fn validate_base_submissions(
    entries: &[ImportEntry],
    base_sections: &HashMap<Uuid, Uuid>,
) -> Result<()> {
    for entry in entries {
        let Some(base_submission_id) = entry.base_submission_id else {
            continue;
        };
        let base_section_id = base_sections.get(&base_submission_id).with_context(|| {
            format!(
                "section '{}' references nonexistent base submission {base_submission_id}",
                entry_label(entry)
            )
        })?;
        ensure!(
            *base_section_id == entry.section_id,
            "section '{}' references base submission {base_submission_id} from another section",
            entry_label(entry)
        );
    }
    Ok(())
}

async fn run(args: Args) -> Result<()> {
    let raw = fs::read_to_string(&args.input_file)
        .with_context(|| format!("failed to read input file '{}'", args.input_file))?;
    let import: ImportFile = toml::from_str(&raw)
        .with_context(|| format!("failed to parse TOML from '{}'", args.input_file))?;

    ensure!(
        matches!(import.format_version, 1 | 2),
        "unsupported import format version {}",
        import.format_version
    );
    ensure!(
        import.user.username.eq_ignore_ascii_case(&args.username),
        "import file is for user '{}' but command target is '{}'",
        import.user.username,
        args.username
    );
    ensure!(!import.entries.is_empty(), "import contains no entries");

    let mut seen = std::collections::HashSet::new();
    for entry in &import.entries {
        ensure!(
            seen.insert(entry.section_id),
            "section '{}' occurs more than once in the import",
            entry_label(entry)
        );
    }

    let config = Config::from_env().context("failed to load configuration")?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&config.database_url)
        .await
        .context("failed to connect to database")?;
    let embedding_client = EmbeddingClient::new(config.embedding_service_url.clone());

    let user = users::find_by_login_identifier(&pool, &args.username)
        .await
        .context("failed to load user")?
        .with_context(|| format!("user '{}' not found", args.username))?;

    for entry in &import.entries {
        let section = sections::find_by_id(&pool, entry.section_id)
            .await
            .with_context(|| format!("failed to load section '{}'", entry_label(entry)))?
            .with_context(|| format!("section '{}' not found", entry_label(entry)))?;
        ensure!(
            section.has_own_text,
            "section '{}' does not accept its own text contributions",
            entry_label(entry)
        );
    }

    let section_ids = import
        .entries
        .iter()
        .map(|entry| entry.section_id)
        .collect::<Vec<_>>();
    let base_submission_ids = import
        .entries
        .iter()
        .filter_map(|entry| entry.base_submission_id)
        .collect::<Vec<_>>();
    let base_sections = sqlx::query_as::<_, (Uuid, Uuid)>(
        "select id, section_id from submissions where id = any($1)",
    )
    .bind(&base_submission_ids)
    .fetch_all(&pool)
    .await
    .context("failed to validate base submissions")?
    .into_iter()
    .collect::<HashMap<_, _>>();
    validate_base_submissions(&import.entries, &base_sections)?;

    let references = submissions::current_references(&pool, &section_ids)
        .await
        .context("failed to load current main submissions")?
        .into_iter()
        .map(|reference| (reference.section_id, reference.submission_id))
        .collect::<HashMap<_, _>>();
    validate_references(&import.entries, &references, import.format_version >= 2)?;

    let action = if args.publish {
        "publish"
    } else {
        "save draft"
    };
    let plan_verb = if args.dry_run { "Would" } else { "Will" };
    for entry in &import.entries {
        println!("{plan_verb} {action}: {}", entry_label(entry));
    }
    if args.dry_run {
        println!(
            "Dry run successful: {} contribution(s) validated; no changes written.",
            import.entries.len()
        );
        return Ok(());
    }

    let mut tx = pool
        .begin()
        .await
        .context("failed to open import transaction")?;
    sections::lock_many_for_update(&mut tx, &section_ids)
        .await
        .context("failed to lock sections for import")?;
    let locked_base_sections = sqlx::query_as::<_, (Uuid, Uuid)>(
        "select id, section_id from submissions where id = any($1)",
    )
    .bind(&base_submission_ids)
    .fetch_all(&mut *tx)
    .await
    .context("failed to recheck base submissions")?
    .into_iter()
    .collect::<HashMap<_, _>>();
    validate_base_submissions(&import.entries, &locked_base_sections)?;

    let locked_references = submissions::current_references_in_tx(&mut tx, &section_ids)
        .await
        .context("failed to recheck current main submissions")?
        .into_iter()
        .map(|reference| (reference.section_id, reference.submission_id))
        .collect::<HashMap<_, _>>();
    validate_references(
        &import.entries,
        &locked_references,
        import.format_version >= 2,
    )?;

    for entry in &import.entries {
        drafts::upsert_in_tx(
            &mut tx,
            &drafts::UpsertDraft {
                id: Uuid::new_v4(),
                section_id: entry.section_id,
                user_id: user.id,
                base_submission_id: entry.base_submission_id,
                markdown_content: entry.draft_markdown.clone(),
                main_comment_markdown: entry.draft_main_comment_markdown.clone(),
            },
        )
        .await
        .with_context(|| format!("failed to import draft for '{}'", entry_label(entry)))?;

        if !args.publish {
            continue;
        }

        let submission = submissions::create(
            &mut tx,
            &submissions::NewSubmission {
                id: Uuid::new_v4(),
                section_id: entry.section_id,
                user_id: user.id,
                base_submission_id: entry.base_submission_id,
                markdown_content: entry.draft_markdown.clone(),
            },
        )
        .await
        .with_context(|| format!("failed to publish '{}'", entry_label(entry)))?;

        submissions::supersede_previous_active_for_user(
            &mut tx,
            entry.section_id,
            user.id,
            submission.id,
        )
        .await
        .with_context(|| format!("failed to supersede previous '{}'", entry_label(entry)))?;

        if let Some(markdown_content) = entry
            .draft_main_comment_markdown
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            comments::create_in_tx(
                &mut tx,
                &comments::NewComment {
                    id: Uuid::new_v4(),
                    submission_id: submission.id,
                    parent_comment_id: None,
                    user_id: user.id,
                    markdown_content: markdown_content.to_owned(),
                    is_primary: true,
                },
            )
            .await
            .with_context(|| {
                format!("failed to create main comment for '{}'", entry_label(entry))
            })?;
        }

        drafts::delete_for_user_in_tx(&mut tx, entry.section_id, user.id)
            .await
            .with_context(|| {
                format!(
                    "failed to remove published draft for '{}'",
                    entry_label(entry)
                )
            })?;
    }

    tx.commit()
        .await
        .context("failed to commit contribution import")?;

    if args.publish {
        for entry in &import.entries {
            if let Err(error) =
                projections::rebuild_trivial_for_section(&pool, &embedding_client, entry.section_id)
                    .await
            {
                bail!(
                    "contributions were committed, but projection rebuild failed for '{}': {error}; do not repeat the import",
                    entry_label(entry)
                );
            }
        }
        println!(
            "Imported and published {} contribution drafts for @{}",
            import.entries.len(),
            user.username
        );
    } else {
        println!(
            "Imported {} contribution drafts for @{}",
            import.entries.len(),
            user.username
        );
    }

    Ok(())
}

#[tokio::main]
async fn main() -> ExitCode {
    let _ = dotenvy::dotenv();
    match run(Args::parse()).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Error: {error:#}");
            ExitCode::from(1)
        }
    }
}

#[cfg(test)]
mod base_tests {
    use super::{ImportEntry, validate_base_submissions};
    use std::collections::HashMap;
    use uuid::Uuid;

    fn entry(section_id: Uuid, base_submission_id: Option<Uuid>) -> ImportEntry {
        ImportEntry {
            section_id,
            section_number: Some("1.2".to_owned()),
            section_title: Some("Example".to_owned()),
            reference_submission_id: None,
            base_submission_id,
            draft_markdown: "Draft".to_owned(),
            draft_main_comment_markdown: None,
        }
    }

    #[test]
    fn base_validation_rejects_missing_submission() {
        let section_id = Uuid::new_v4();
        let base_id = Uuid::new_v4();
        let error = validate_base_submissions(&[entry(section_id, Some(base_id))], &HashMap::new())
            .expect_err("missing base must be rejected");

        assert!(error.to_string().contains("nonexistent base submission"));
    }

    #[test]
    fn base_validation_rejects_submission_from_another_section() {
        let section_id = Uuid::new_v4();
        let base_id = Uuid::new_v4();
        let base_sections = HashMap::from([(base_id, Uuid::new_v4())]);
        let error = validate_base_submissions(&[entry(section_id, Some(base_id))], &base_sections)
            .expect_err("cross-section base must be rejected");

        assert!(error.to_string().contains("from another section"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(reference_submission_id: Option<Uuid>) -> ImportEntry {
        ImportEntry {
            section_id: Uuid::new_v4(),
            section_number: Some("1.2".to_owned()),
            section_title: Some("Section".to_owned()),
            reference_submission_id,
            base_submission_id: None,
            draft_markdown: "Text".to_owned(),
            draft_main_comment_markdown: None,
        }
    }

    #[test]
    fn changed_reference_is_rejected() {
        let expected = Uuid::new_v4();
        let current = Uuid::new_v4();
        let entry = entry(Some(expected));
        let references = HashMap::from([(entry.section_id, current)]);

        let error = validate_references(&[entry], &references, true).expect_err("stale reference");

        assert!(error.to_string().contains("changed since export"));
    }

    #[test]
    fn legacy_entry_without_reference_remains_importable() {
        let entry = entry(None);
        let references = HashMap::from([(entry.section_id, Uuid::new_v4())]);

        validate_references(&[entry], &references, false).expect("legacy entry accepted");
    }

    #[test]
    fn new_main_is_rejected_when_version_two_export_had_none() {
        let entry = entry(None);
        let references = HashMap::from([(entry.section_id, Uuid::new_v4())]);

        let error = validate_references(&[entry], &references, true).expect_err("new main");

        assert!(error.to_string().contains("expected main submission none"));
    }
}
