use std::{
    collections::{BTreeMap, HashMap},
    fs,
    process::ExitCode,
};

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{FromRow, postgres::PgPoolOptions};
use uuid::Uuid;

use rimbun_api::{config::Config, db::users};

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
    section_title: String,
    has_heading: bool,
    document_slug: String,
    document_title: String,
    base_submission_id: Option<Uuid>,
    markdown_content: String,
    main_comment_markdown: Option<String>,
}

#[derive(Debug, Clone, FromRow)]
struct ActiveSubmissionRow {
    section_id: Uuid,
    section_title: String,
    has_heading: bool,
    document_slug: String,
    document_title: String,
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
    parent_id: Option<Uuid>,
    title: String,
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

fn usage() {
    eprintln!(
        "Usage: cargo run -p rimbun-api --bin rimbun-export-user-contributions -- <username> [output-file]"
    );
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
            parent_id,
            title: title.to_owned(),
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
}

#[tokio::main]
async fn main() -> ExitCode {
    let _ = dotenvy::dotenv();

    let mut args = std::env::args().skip(1);
    let Some(username) = args.next() else {
        usage();
        return ExitCode::from(2);
    };
    let output_file = args.next();
    if args.next().is_some() {
        usage();
        return ExitCode::from(2);
    }

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

    let user = match users::find_by_login_identifier(&pool, &username).await {
        Ok(Some(user)) => user,
        Ok(None) => {
            eprintln!("Error: user '{username}' not found");
            return ExitCode::from(1);
        }
        Err(error) => {
            eprintln!("Error: failed to load user: {error}");
            return ExitCode::from(1);
        }
    };

    let drafts = match sqlx::query_as::<_, DraftRow>(
        r#"
        select
          s.id as section_id,
          s.title as section_title,
          s.has_heading,
          d.slug as document_slug,
          d.title as document_title,
          dr.base_submission_id,
          dr.markdown_content,
          dr.main_comment_markdown
        from drafts dr
        join sections s on s.id = dr.section_id
        join documents d on d.id = s.document_id
        where dr.user_id = $1
          and s.has_own_text = true
        order by d.slug asc, s.path asc, s.position asc
        "#,
    )
    .bind(user.id)
    .fetch_all(&pool)
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
          s.title as section_title,
          s.has_heading,
          d.slug as document_slug,
          d.title as document_title,
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
        join documents d on d.id = s.document_id
        where sub.user_id = $1
          and sub.superseded_by is null
          and s.has_own_text = true
        order by d.slug asc, s.path asc, s.position asc
        "#,
    )
    .bind(user.id)
    .fetch_all(&pool)
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
    .fetch_all(&pool)
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
        select id, document_id, parent_id, title, position, created_at
        from sections
        "#,
    )
    .fetch_all(&pool)
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

    let preference_map: HashMap<Uuid, Uuid> = preferences
        .into_iter()
        .map(|row| (row.section_id, row.preferred_base_submission_id))
        .collect();

    let mut entries = BTreeMap::<(String, String, Uuid), EntryBuilder>::new();

    for row in active_submissions {
        let section_breadcrumb = breadcrumb_for_section(row.section_id, &parents, &titles);
        let section_path = section_breadcrumb.join(" / ");
        let key = (
            row.document_slug.clone(),
            section_path.clone(),
            row.section_id,
        );
        let entry = entries.entry(key).or_insert_with(|| {
            EntryBuilder::new(EntrySection {
                document_slug: row.document_slug.clone(),
                document_title: row.document_title.clone(),
                section_id: row.section_id,
                section_number: section_numbers
                    .get(&row.section_id)
                    .cloned()
                    .unwrap_or_default(),
                section_breadcrumb,
                section_path,
                section_title: row.section_title.clone(),
                has_heading: row.has_heading,
            })
        });

        entry.published_submission_id = Some(row.submission_id);
        entry.published_markdown = Some(row.markdown_content.clone());
        entry.published_main_comment_markdown = row.published_main_comment_markdown.clone();
        entry.published_at = Some(row.published_at);

        if entry.draft_source == "empty" {
            entry.draft_source = "published".to_owned();
            entry.base_submission_id = row.base_submission_id;
            entry.draft_markdown = row.markdown_content;
            entry.draft_main_comment_markdown = row.published_main_comment_markdown;
        }
    }

    for row in drafts {
        let section_breadcrumb = breadcrumb_for_section(row.section_id, &parents, &titles);
        let section_path = section_breadcrumb.join(" / ");
        let key = (
            row.document_slug.clone(),
            section_path.clone(),
            row.section_id,
        );
        let entry = entries.entry(key).or_insert_with(|| {
            EntryBuilder::new(EntrySection {
                document_slug: row.document_slug.clone(),
                document_title: row.document_title.clone(),
                section_id: row.section_id,
                section_number: section_numbers
                    .get(&row.section_id)
                    .cloned()
                    .unwrap_or_default(),
                section_breadcrumb,
                section_path,
                section_title: row.section_title.clone(),
                has_heading: row.has_heading,
            })
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

    let export = ExportFile {
        format_version: 1,
        exported_at: Utc::now(),
        user: ExportUser {
            username: user.username,
            display_name: user.display_name,
            email: user.email,
            role: user.role,
        },
        entries: entries.into_values().map(EntryBuilder::build).collect(),
    };

    let rendered = match toml::to_string_pretty(&export) {
        Ok(rendered) => rendered,
        Err(error) => {
            eprintln!("Error: failed to serialize export: {error}");
            return ExitCode::from(1);
        }
    };

    match output_file {
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
