use std::{fs, process::ExitCode};

use serde::Deserialize;
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

use rimbun_api::{
    config::Config,
    db::{drafts, projections, sections, submissions, users},
};
use rimbun_embedding_client::EmbeddingClient;

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

#[derive(Debug, Deserialize)]
struct ImportEntry {
    section_id: Uuid,
    base_submission_id: Option<Uuid>,
    draft_markdown: String,
}

fn usage() {
    eprintln!(
        "Usage: cargo run -p rimbun-api --bin rimbun-import-user-contributions -- <username> <input-file> [--publish]"
    );
}

#[tokio::main]
async fn main() -> ExitCode {
    let _ = dotenvy::dotenv();

    let mut args = std::env::args().skip(1);
    let Some(username) = args.next() else {
        usage();
        return ExitCode::from(2);
    };
    let Some(input_file) = args.next() else {
        usage();
        return ExitCode::from(2);
    };
    let mut publish = false;
    for arg in args {
        if arg == "--publish" {
            publish = true;
        } else {
            usage();
            return ExitCode::from(2);
        }
    }

    let raw = match fs::read_to_string(&input_file) {
        Ok(raw) => raw,
        Err(error) => {
            eprintln!("Error: failed to read input file '{input_file}': {error}");
            return ExitCode::from(1);
        }
    };

    let import: ImportFile = match toml::from_str(&raw) {
        Ok(import) => import,
        Err(error) => {
            eprintln!("Error: failed to parse TOML from '{input_file}': {error}");
            return ExitCode::from(1);
        }
    };

    if import.format_version != 1 {
        eprintln!(
            "Error: unsupported import format version {}",
            import.format_version
        );
        return ExitCode::from(2);
    }

    if !import.user.username.eq_ignore_ascii_case(&username) {
        eprintln!(
            "Error: import file is for user '{}' but command target is '{}'",
            import.user.username, username
        );
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
    let embedding_client = EmbeddingClient::new(config.embedding_service_url.clone());

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

    let mut imported = 0usize;

    for entry in import.entries {
        let ImportEntry {
            section_id,
            base_submission_id,
            draft_markdown,
        } = entry;

        let section = match sections::find_by_id(&pool, section_id).await {
            Ok(Some(section)) => section,
            Ok(None) => {
                eprintln!("Error: section '{}' not found", section_id);
                return ExitCode::from(1);
            }
            Err(error) => {
                eprintln!("Error: failed to load section '{}': {error}", section_id);
                return ExitCode::from(1);
            }
        };

        if !section.has_own_text {
            eprintln!(
                "Error: section '{}' does not accept its own text contributions",
                section_id
            );
            return ExitCode::from(1);
        }

        let draft = drafts::UpsertDraft {
            id: Uuid::new_v4(),
            section_id,
            user_id: user.id,
            base_submission_id,
            markdown_content: draft_markdown.clone(),
        };

        if let Err(error) = drafts::upsert(&pool, &draft).await {
            eprintln!(
                "Error: failed to import draft for section '{}': {error}",
                section_id
            );
            return ExitCode::from(1);
        }

        if publish {
            let mut tx = match pool.begin().await {
                Ok(tx) => tx,
                Err(error) => {
                    eprintln!(
                        "Error: failed to open transaction for section '{}': {error}",
                        section_id
                    );
                    return ExitCode::from(1);
                }
            };

            let submission = match submissions::create(
                &mut tx,
                &submissions::NewSubmission {
                    id: Uuid::new_v4(),
                    section_id,
                    user_id: user.id,
                    base_submission_id,
                    markdown_content: draft_markdown.clone(),
                },
            )
            .await
            {
                Ok(submission) => submission,
                Err(error) => {
                    eprintln!(
                        "Error: failed to create submission for section '{}': {error}",
                        section_id
                    );
                    return ExitCode::from(1);
                }
            };

            if let Err(error) = submissions::supersede_previous_active_for_user(
                &mut tx,
                section_id,
                user.id,
                submission.id,
            )
            .await
            {
                eprintln!(
                    "Error: failed to supersede previous submission for section '{}': {error}",
                    section_id
                );
                return ExitCode::from(1);
            }

            if let Err(error) = tx.commit().await {
                eprintln!(
                    "Error: failed to commit published import for section '{}': {error}",
                    section_id
                );
                return ExitCode::from(1);
            }

            if let Err(error) =
                projections::rebuild_trivial_for_section(&pool, &embedding_client, section_id)
                    .await
            {
                eprintln!(
                    "Error: failed to rebuild projection for section '{}': {error}",
                    section_id
                );
                return ExitCode::from(1);
            }
        }

        imported += 1;
    }

    if publish {
        println!(
            "Imported and published {imported} contribution drafts for @{}",
            user.username
        );
    } else {
        println!("Imported {imported} contribution drafts for @{}", user.username);
    }
    ExitCode::SUCCESS
}
