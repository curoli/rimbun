use std::{
    collections::{HashMap, HashSet},
    fs,
    process::ExitCode,
};

use chrono::{DateTime, Utc};
use clap::Parser;
use serde::Serialize;
use sqlx::{FromRow, postgres::PgPoolOptions};
use uuid::Uuid;

use rimbun_api::{
    config::Config,
    db::{submissions, users},
};

#[derive(Debug, Parser)]
struct Args {
    username: String,
    output_file: Option<String>,
    #[arg(long)]
    document: Option<String>,
    #[arg(long, requires = "document")]
    section: Option<String>,
    #[arg(long, requires = "section")]
    recursive: bool,
    #[arg(long)]
    include_empty: bool,
}

#[derive(Debug, Serialize)]
struct ExportFile {
    format_version: u32,
    exported_at: DateTime<Utc>,
    user: ExportUser,
    entries: Vec<ExportEntry>,
}

#[derive(Debug, Serialize)]
struct ExportUser {
    username: String,
    display_name: String,
    email: String,
    role: String,
}

#[derive(Debug, Serialize)]
struct ExportEntry {
    document_slug: String,
    document_title: String,
    section_id: Uuid,
    section_number: String,
    section_breadcrumb: Vec<String>,
    section_path: String,
    section_title: String,
    has_heading: bool,
    reference_submission_id: Option<Uuid>,
    reference_markdown: Option<String>,
    draft_source: String,
    base_submission_id: Option<Uuid>,
    draft_markdown: String,
    draft_main_comment_markdown: Option<String>,
    published_submission_id: Option<Uuid>,
    published_markdown: Option<String>,
    published_main_comment_markdown: Option<String>,
    published_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, FromRow)]
struct DraftRow {
    section_id: Uuid,
    base_submission_id: Option<Uuid>,
    markdown_content: String,
    main_comment_markdown: Option<String>,
}

#[derive(Debug, Clone, FromRow)]
struct ActiveSubmissionRow {
    section_id: Uuid,
    submission_id: Uuid,
    base_submission_id: Option<Uuid>,
    markdown_content: String,
    published_main_comment_markdown: Option<String>,
    published_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow)]
struct PreferenceRow {
    section_id: Uuid,
    preferred_base_submission_id: Uuid,
}

#[derive(Debug, Clone, FromRow)]
struct SectionMeta {
    id: Uuid,
    document_id: Uuid,
    document_slug: String,
    document_title: String,
    parent_id: Option<Uuid>,
    title: String,
    has_heading: bool,
    has_own_text: bool,
    position: i32,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
struct EntrySection {
    document_slug: String,
    document_title: String,
    section_id: Uuid,
    section_number: String,
    section_breadcrumb: Vec<String>,
    section_path: String,
    section_title: String,
    has_heading: bool,
    reference_submission_id: Option<Uuid>,
    reference_markdown: Option<String>,
}

#[derive(Debug, Clone)]
struct EntryBuilder {
    document_slug: String,
    document_title: String,
    section_id: Uuid,
    section_number: String,
    section_breadcrumb: Vec<String>,
    section_path: String,
    section_title: String,
    has_heading: bool,
    reference_submission_id: Option<Uuid>,
    reference_markdown: Option<String>,
    draft_source: String,
    base_submission_id: Option<Uuid>,
    draft_markdown: String,
    draft_main_comment_markdown: Option<String>,
    published_submission_id: Option<Uuid>,
    published_markdown: Option<String>,
    published_main_comment_markdown: Option<String>,
    published_at: Option<DateTime<Utc>>,
}

impl EntryBuilder {
    fn new(section: EntrySection) -> Self {
        Self {
            document_slug: section.document_slug,
            document_title: section.document_title,
            section_id: section.section_id,
            section_number: section.section_number,
            section_breadcrumb: section.section_breadcrumb,
            section_path: section.section_path,
            section_title: section.section_title,
            has_heading: section.has_heading,
            reference_submission_id: section.reference_submission_id,
            reference_markdown: section.reference_markdown,
            draft_source: "empty".to_owned(),
            base_submission_id: None,
            draft_markdown: String::new(),
            draft_main_comment_markdown: None,
            published_submission_id: None,
            published_markdown: None,
            published_main_comment_markdown: None,
            published_at: None,
        }
    }

    fn build(self) -> ExportEntry {
        ExportEntry {
            document_slug: self.document_slug,
            document_title: self.document_title,
            section_id: self.section_id,
            section_number: self.section_number,
            section_breadcrumb: self.section_breadcrumb,
            section_path: self.section_path,
            section_title: self.section_title,
            has_heading: self.has_heading,
            reference_submission_id: self.reference_submission_id,
            reference_markdown: self.reference_markdown,
            draft_source: self.draft_source,
            base_submission_id: self.base_submission_id,
            draft_markdown: self.draft_markdown,
            draft_main_comment_markdown: self.draft_main_comment_markdown,
            published_submission_id: self.published_submission_id,
            published_markdown: self.published_markdown,
            published_main_comment_markdown: self.published_main_comment_markdown,
            published_at: self.published_at,
        }
    }
}

fn breadcrumb_for_section(
    section_id: Uuid,
    parents: &HashMap<Uuid, Option<Uuid>>,
    titles: &HashMap<Uuid, String>,
) -> Vec<String> {
    let mut cursor = Some(section_id);
    let mut parts = Vec::new();

    while let Some(current) = cursor {
        if let Some(title) = titles.get(&current)
            && !title.is_empty()
        {
            parts.push(title.clone());
        }
        cursor = parents.get(&current).copied().flatten();
    }

    parts.reverse();
    parts
}

fn section_numbers(sections: &[SectionMeta]) -> HashMap<Uuid, String> {
    let mut roots = HashMap::<Uuid, Vec<&SectionMeta>>::new();
    let mut children = HashMap::<Uuid, Vec<&SectionMeta>>::new();

    for section in sections {
        if let Some(parent_id) = section.parent_id {
            children.entry(parent_id).or_default().push(section);
        } else {
            roots.entry(section.document_id).or_default().push(section);
        }
    }

    let sort_sections = |group: &mut Vec<&SectionMeta>| {
        group.sort_by(|left, right| {
            left.position
                .cmp(&right.position)
                .then(left.created_at.cmp(&right.created_at))
        });
    };
    for group in roots.values_mut() {
        sort_sections(group);
    }
    for group in children.values_mut() {
        sort_sections(group);
    }

    fn visit(
        siblings: &[&SectionMeta],
        children: &HashMap<Uuid, Vec<&SectionMeta>>,
        prefix: &mut Vec<usize>,
        numbers: &mut HashMap<Uuid, String>,
    ) {
        for (index, section) in siblings.iter().enumerate() {
            prefix.push(index + 1);
            numbers.insert(
                section.id,
                prefix
                    .iter()
                    .map(|part| part.to_string())
                    .collect::<Vec<_>>()
                    .join("."),
            );
            if let Some(descendants) = children.get(&section.id) {
                visit(descendants, children, prefix, numbers);
            }
            prefix.pop();
        }
    }

    let mut numbers = HashMap::new();
    for root_sections in roots.values() {
        visit(root_sections, &children, &mut Vec::new(), &mut numbers);
    }
    numbers
}

fn select_sections(
    sections: &[SectionMeta],
    numbers: &HashMap<Uuid, String>,
    document: Option<&str>,
    section: Option<&str>,
    recursive: bool,
) -> Result<HashSet<Uuid>, String> {
    let in_document = sections
        .iter()
        .filter(|candidate| document.is_none_or(|slug| candidate.document_slug == slug))
        .collect::<Vec<_>>();

    if let Some(slug) = document
        && in_document.is_empty()
    {
        return Err(format!("document '{slug}' not found"));
    }

    let selected_root = if let Some(selector) = section {
        let selector_id = Uuid::parse_str(selector).ok();
        Some(
            in_document
                .iter()
                .find(|candidate| {
                    selector_id == Some(candidate.id)
                        || numbers
                            .get(&candidate.id)
                            .is_some_and(|number| number == selector)
                })
                .copied()
                .ok_or_else(|| {
                    format!("section '{selector}' not found in the selected document")
                })?,
        )
    } else {
        None
    };

    let selected = in_document
        .into_iter()
        .filter(|candidate| candidate.has_own_text)
        .filter(|candidate| {
            let Some(root) = selected_root else {
                return true;
            };
            if candidate.id == root.id {
                return true;
            }
            recursive
                && numbers
                    .get(&candidate.id)
                    .zip(numbers.get(&root.id))
                    .is_some_and(|(candidate_number, root_number)| {
                        candidate_number.starts_with(&format!("{root_number}."))
                    })
        })
        .map(|section| section.id)
        .collect::<HashSet<_>>();

    if selected.is_empty() {
        return Err("the selected scope contains no sections that accept contributions".to_owned());
    }

    Ok(selected)
}

fn entry_section(
    section: &SectionMeta,
    numbers: &HashMap<Uuid, String>,
    parents: &HashMap<Uuid, Option<Uuid>>,
    titles: &HashMap<Uuid, String>,
    references: &HashMap<Uuid, submissions::ReferenceSubmission>,
) -> EntrySection {
    let section_breadcrumb = breadcrumb_for_section(section.id, parents, titles);
    let reference = references.get(&section.id);
    EntrySection {
        document_slug: section.document_slug.clone(),
        document_title: section.document_title.clone(),
        section_id: section.id,
        section_number: numbers.get(&section.id).cloned().unwrap_or_default(),
        section_path: section_breadcrumb.join(" / "),
        section_breadcrumb,
        section_title: section.title.clone(),
        has_heading: section.has_heading,
        reference_submission_id: reference.map(|entry| entry.submission_id),
        reference_markdown: reference.map(|entry| entry.markdown_content.clone()),
    }
}

fn section_number_parts(number: &str) -> Vec<usize> {
    number
        .split('.')
        .filter_map(|part| part.parse().ok())
        .collect()
}

#[tokio::main]
async fn main() -> ExitCode {
    let _ = dotenvy::dotenv();
    let args = Args::parse();

    let config = match Config::from_env() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("Error: failed to load configuration: {error}");
            return ExitCode::from(1);
        }
    };

    let pool = match PgPoolOptions::new()
        .max_connections(1)
        .connect(&config.database_url)
        .await
    {
        Ok(pool) => pool,
        Err(error) => {
            eprintln!("Error: failed to connect to database: {error}");
            return ExitCode::from(1);
        }
    };

    let user = match users::find_by_login_identifier(&pool, &args.username).await {
        Ok(Some(user)) => user,
        Ok(None) => {
            eprintln!("Error: user '{}' not found", args.username);
            return ExitCode::from(1);
        }
        Err(error) => {
            eprintln!("Error: failed to load user: {error}");
            return ExitCode::from(1);
        }
    };

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(error) => {
            eprintln!("Error: failed to start export snapshot: {error}");
            return ExitCode::from(1);
        }
    };
    if let Err(error) = sqlx::query("set transaction isolation level repeatable read read only")
        .execute(&mut *tx)
        .await
    {
        eprintln!("Error: failed to configure export snapshot: {error}");
        return ExitCode::from(1);
    }

    let drafts = match sqlx::query_as::<_, DraftRow>(
        r#"
        select
          s.id as section_id,
          dr.base_submission_id,
          dr.markdown_content,
          dr.main_comment_markdown
        from drafts dr
        join sections s on s.id = dr.section_id
        where dr.user_id = $1
          and s.has_own_text = true
        "#,
    )
    .bind(user.id)
    .fetch_all(&mut *tx)
    .await
    {
        Ok(rows) => rows,
        Err(error) => {
            eprintln!("Error: failed to load drafts: {error}");
            return ExitCode::from(1);
        }
    };

    let active_submissions = match sqlx::query_as::<_, ActiveSubmissionRow>(
        r#"
        select
          s.id as section_id,
          sub.id as submission_id,
          sub.base_submission_id,
          sub.markdown_content,
          (
            select c.markdown_content
            from comments c
            where c.submission_id = sub.id
              and c.user_id = sub.user_id
              and c.is_primary = true
              and c.deleted_at is null
              and c.parent_comment_id is null
            limit 1
          ) as published_main_comment_markdown,
          sub.published_at
        from submissions sub
        join sections s on s.id = sub.section_id
        where sub.user_id = $1
          and sub.superseded_by is null
          and s.has_own_text = true
        "#,
    )
    .bind(user.id)
    .fetch_all(&mut *tx)
    .await
    {
        Ok(rows) => rows,
        Err(error) => {
            eprintln!("Error: failed to load published contributions: {error}");
            return ExitCode::from(1);
        }
    };

    let preferences = match sqlx::query_as::<_, PreferenceRow>(
        r#"
        select section_id, preferred_base_submission_id
        from user_section_preferences
        where user_id = $1
        "#,
    )
    .bind(user.id)
    .fetch_all(&mut *tx)
    .await
    {
        Ok(rows) => rows,
        Err(error) => {
            eprintln!("Error: failed to load preferences: {error}");
            return ExitCode::from(1);
        }
    };

    let section_meta_rows = match sqlx::query_as::<_, SectionMeta>(
        r#"
        select
          s.id,
          s.document_id,
          d.slug as document_slug,
          d.title as document_title,
          s.parent_id,
          s.title,
          s.has_heading,
          s.has_own_text,
          s.position,
          s.created_at
        from sections s
        join documents d on d.id = s.document_id
        "#,
    )
    .fetch_all(&mut *tx)
    .await
    {
        Ok(rows) => rows,
        Err(error) => {
            eprintln!("Error: failed to load section metadata: {error}");
            return ExitCode::from(1);
        }
    };

    let mut parents = HashMap::new();
    let mut titles = HashMap::new();
    for section in &section_meta_rows {
        parents.insert(section.id, section.parent_id);
        titles.insert(section.id, section.title.clone());
    }
    let section_numbers = section_numbers(&section_meta_rows);
    let selected_sections = match select_sections(
        &section_meta_rows,
        &section_numbers,
        args.document.as_deref(),
        args.section.as_deref(),
        args.recursive,
    ) {
        Ok(selected) => selected,
        Err(error) => {
            eprintln!("Error: {error}");
            return ExitCode::from(2);
        }
    };
    let selected_section_ids = selected_sections.iter().copied().collect::<Vec<_>>();
    let references =
        match submissions::current_references_in_tx(&mut tx, &selected_section_ids).await {
            Ok(rows) => rows
                .into_iter()
                .map(|reference| (reference.section_id, reference))
                .collect::<HashMap<_, _>>(),
            Err(error) => {
                eprintln!("Error: failed to load current main submissions: {error}");
                return ExitCode::from(1);
            }
        };
    if let Err(error) = tx.commit().await {
        eprintln!("Error: failed to complete export snapshot: {error}");
        return ExitCode::from(1);
    }
    let sections_by_id = section_meta_rows
        .iter()
        .map(|section| (section.id, section))
        .collect::<HashMap<_, _>>();

    let preference_map: HashMap<Uuid, Uuid> = preferences
        .into_iter()
        .map(|row| (row.section_id, row.preferred_base_submission_id))
        .collect();

    let mut entries = HashMap::<Uuid, EntryBuilder>::new();

    if args.include_empty {
        for section_id in &selected_sections {
            let section = sections_by_id
                .get(section_id)
                .expect("selected section metadata exists");
            let mut entry = EntryBuilder::new(entry_section(
                section,
                &section_numbers,
                &parents,
                &titles,
                &references,
            ));
            if let Some(reference) = references.get(section_id) {
                entry.draft_source = "reference".to_owned();
                entry.base_submission_id = Some(reference.submission_id);
                entry.draft_markdown = reference.markdown_content.clone();
            }
            entries.insert(*section_id, entry);
        }
    }

    for row in active_submissions {
        if !selected_sections.contains(&row.section_id) {
            continue;
        }
        let section = sections_by_id
            .get(&row.section_id)
            .expect("submission section metadata exists");
        let entry = entries.entry(row.section_id).or_insert_with(|| {
            EntryBuilder::new(entry_section(
                section,
                &section_numbers,
                &parents,
                &titles,
                &references,
            ))
        });

        entry.published_submission_id = Some(row.submission_id);
        entry.published_markdown = Some(row.markdown_content.clone());
        entry.published_main_comment_markdown = row.published_main_comment_markdown.clone();
        entry.published_at = Some(row.published_at);

        if matches!(entry.draft_source.as_str(), "empty" | "reference") {
            entry.draft_source = "published".to_owned();
            entry.base_submission_id = row.base_submission_id;
            entry.draft_markdown = row.markdown_content;
            entry.draft_main_comment_markdown = row.published_main_comment_markdown;
        }
    }

    for row in drafts {
        if !selected_sections.contains(&row.section_id) {
            continue;
        }
        let section = sections_by_id
            .get(&row.section_id)
            .expect("draft section metadata exists");
        let entry = entries.entry(row.section_id).or_insert_with(|| {
            EntryBuilder::new(entry_section(
                section,
                &section_numbers,
                &parents,
                &titles,
                &references,
            ))
        });

        entry.draft_source = "draft".to_owned();
        entry.base_submission_id = row.base_submission_id;
        entry.draft_markdown = row.markdown_content;
        entry.draft_main_comment_markdown = row.main_comment_markdown;
    }

    for entry in entries.values_mut() {
        if entry.base_submission_id.is_none() {
            entry.base_submission_id = preference_map.get(&entry.section_id).copied();
        }
    }

    let mut export_entries = entries
        .into_values()
        .map(EntryBuilder::build)
        .collect::<Vec<_>>();
    export_entries.sort_by(|left, right| {
        left.document_slug.cmp(&right.document_slug).then_with(|| {
            section_number_parts(&left.section_number)
                .cmp(&section_number_parts(&right.section_number))
        })
    });

    let export = ExportFile {
        format_version: 2,
        exported_at: Utc::now(),
        user: ExportUser {
            username: user.username,
            display_name: user.display_name,
            email: user.email,
            role: user.role,
        },
        entries: export_entries,
    };

    let rendered = match toml::to_string_pretty(&export) {
        Ok(rendered) => rendered,
        Err(error) => {
            eprintln!("Error: failed to serialize export: {error}");
            return ExitCode::from(1);
        }
    };

    match args.output_file {
        Some(path) => match fs::write(&path, rendered) {
            Ok(()) => {
                println!(
                    "Exported {} contribution entries to {}",
                    export.entries.len(),
                    path
                );
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("Error: failed to write export file '{path}': {error}");
                ExitCode::from(1)
            }
        },
        None => {
            print!("{rendered}");
            ExitCode::SUCCESS
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn section(
        id: Uuid,
        document_id: Uuid,
        parent_id: Option<Uuid>,
        title: &str,
        position: i32,
        created_at: &str,
    ) -> SectionMeta {
        SectionMeta {
            id,
            document_id,
            document_slug: format!("document-{document_id}"),
            document_title: "Document".to_owned(),
            parent_id,
            title: title.to_owned(),
            has_heading: true,
            has_own_text: true,
            position,
            created_at: created_at.parse().expect("valid test timestamp"),
        }
    }

    #[test]
    fn section_numbers_match_reader_hierarchy_and_ordering() {
        let document_id = Uuid::new_v4();
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        let first_child = Uuid::new_v4();
        let second_child = Uuid::new_v4();
        let sections = vec![
            section(
                second_child,
                document_id,
                Some(first),
                "Second child",
                1,
                "2026-01-01T00:00:04Z",
            ),
            section(
                second,
                document_id,
                None,
                "Second",
                1,
                "2026-01-01T00:00:02Z",
            ),
            section(
                first_child,
                document_id,
                Some(first),
                "First child",
                0,
                "2026-01-01T00:00:03Z",
            ),
            section(first, document_id, None, "First", 0, "2026-01-01T00:00:01Z"),
        ];

        let numbers = section_numbers(&sections);

        assert_eq!(numbers.get(&first).map(String::as_str), Some("1"));
        assert_eq!(numbers.get(&first_child).map(String::as_str), Some("1.1"));
        assert_eq!(numbers.get(&second_child).map(String::as_str), Some("1.2"));
        assert_eq!(numbers.get(&second).map(String::as_str), Some("2"));
    }

    #[test]
    fn root_numbering_is_independent_per_document() {
        let first_document = Uuid::new_v4();
        let second_document = Uuid::new_v4();
        let first_section = Uuid::new_v4();
        let second_section = Uuid::new_v4();
        let sections = vec![
            section(
                first_section,
                first_document,
                None,
                "First document",
                0,
                "2026-01-01T00:00:01Z",
            ),
            section(
                second_section,
                second_document,
                None,
                "Second document",
                0,
                "2026-01-01T00:00:02Z",
            ),
        ];

        let numbers = section_numbers(&sections);

        assert_eq!(numbers.get(&first_section).map(String::as_str), Some("1"));
        assert_eq!(numbers.get(&second_section).map(String::as_str), Some("1"));
    }

    #[test]
    fn breadcrumb_contains_all_non_empty_ancestor_titles() {
        let root = Uuid::new_v4();
        let untitled = Uuid::new_v4();
        let leaf = Uuid::new_v4();
        let parents = HashMap::from([(root, None), (untitled, Some(root)), (leaf, Some(untitled))]);
        let titles = HashMap::from([
            (root, "Book".to_owned()),
            (untitled, String::new()),
            (leaf, "Chapter".to_owned()),
        ]);

        assert_eq!(
            breadcrumb_for_section(leaf, &parents, &titles),
            vec!["Book", "Chapter"]
        );
    }

    #[test]
    fn selection_supports_numbered_recursive_document_scope() {
        let document_id = Uuid::new_v4();
        let root = Uuid::new_v4();
        let child = Uuid::new_v4();
        let sibling = Uuid::new_v4();
        let sections = vec![
            section(root, document_id, None, "Root", 0, "2026-01-01T00:00:01Z"),
            section(
                child,
                document_id,
                Some(root),
                "Child",
                0,
                "2026-01-01T00:00:02Z",
            ),
            section(
                sibling,
                document_id,
                None,
                "Sibling",
                1,
                "2026-01-01T00:00:03Z",
            ),
        ];
        let numbers = section_numbers(&sections);
        let document_slug = format!("document-{document_id}");

        let selected = select_sections(&sections, &numbers, Some(&document_slug), Some("1"), true)
            .expect("valid selection");

        assert_eq!(selected, HashSet::from([root, child]));
    }
}
