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

#[derive(Debug, Clone)]
struct EntryBuilder {
    document_slug: String,
    document_title: String,
    section_id: Uuid,
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
    fn new(
        document_slug: String,
        document_title: String,
        section_id: Uuid,
        section_path: String,
        section_title: String,
        has_heading: bool,
    ) -> Self {
        Self {
            document_slug,
            document_title,
            section_id,
            section_path,
            section_title,
            has_heading,
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

fn title_path_for_section(
    section_id: Uuid,
    parents: &HashMap<Uuid, Option<Uuid>>,
    titles: &HashMap<Uuid, String>,
) -> String {
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
    parts.join(" / ")
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

    let section_meta_rows = match sqlx::query_as::<_, (Uuid, Option<Uuid>, String)>(
        r#"
        select id, parent_id, title
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
    for (id, parent_id, title) in section_meta_rows {
        parents.insert(id, parent_id);
        titles.insert(id, title);
    }

    let preference_map: HashMap<Uuid, Uuid> = preferences
        .into_iter()
        .map(|row| (row.section_id, row.preferred_base_submission_id))
        .collect();

    let mut entries = BTreeMap::<(String, String, Uuid), EntryBuilder>::new();

    for row in active_submissions {
        let key = (
            row.document_slug.clone(),
            title_path_for_section(row.section_id, &parents, &titles),
            row.section_id,
        );
        let section_path = title_path_for_section(row.section_id, &parents, &titles);
        let entry = entries.entry(key).or_insert_with(|| {
            EntryBuilder::new(
                row.document_slug.clone(),
                row.document_title.clone(),
                row.section_id,
                section_path,
                row.section_title.clone(),
                row.has_heading,
            )
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
        let key = (
            row.document_slug.clone(),
            title_path_for_section(row.section_id, &parents, &titles),
            row.section_id,
        );
        let section_path = title_path_for_section(row.section_id, &parents, &titles);
        let entry = entries.entry(key).or_insert_with(|| {
            EntryBuilder::new(
                row.document_slug.clone(),
                row.document_title.clone(),
                row.section_id,
                section_path,
                row.section_title.clone(),
                row.has_heading,
            )
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
