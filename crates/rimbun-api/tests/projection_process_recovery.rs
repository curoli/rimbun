use std::{fs, path::PathBuf, process::Stdio, time::Duration};

use anyhow::{Context, Result, ensure};
use axum::{Json, Router, routing::post};
use reqwest::Client;
use serde_json::{Value, json};
use sqlx::{PgPool, postgres::PgPoolOptions};
use tokio::{net::TcpListener, process::Child, sync::mpsc, task::JoinHandle};
use uuid::Uuid;

struct TestFiles(PathBuf);

impl TestFiles {
    fn new() -> Result<Self> {
        let path = std::env::temp_dir().join(format!("rimbun-recovery-{}", Uuid::new_v4()));
        fs::create_dir(&path)?;
        Ok(Self(path))
    }

    fn diagnostics(&self) -> String {
        ["before-crash.log", "after-restart.log"]
            .into_iter()
            .filter_map(|name| {
                fs::read_to_string(self.0.join(name))
                    .ok()
                    .map(|contents| format!("{name}:\n{contents}"))
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

impl Drop for TestFiles {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct Backend {
    child: Child,
    endpoint: String,
}

impl Backend {
    async fn start(database_url: &str, embedding_url: &str, log: PathBuf) -> Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let port = listener.local_addr()?.port();
        drop(listener);
        let output = fs::File::create(log)?;
        let child = tokio::process::Command::new(env!("CARGO_BIN_EXE_rimbun-api"))
            .env("DATABASE_URL", database_url)
            .env("SESSION_SECRET", "process-recovery-test")
            .env("EMBEDDING_SERVICE_URL", embedding_url)
            .env("RIMBUN_PORT", port.to_string())
            .env("RUST_LOG", "rimbun_api=info")
            .stdin(Stdio::null())
            .stdout(output.try_clone()?)
            .stderr(output)
            .kill_on_drop(true)
            .spawn()
            .context("start actual backend executable")?;
        Ok(Self {
            child,
            endpoint: format!("http://127.0.0.1:{port}"),
        })
    }

    async fn ready(&mut self, client: &Client) -> Result<()> {
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                ensure!(
                    self.child.try_wait()?.is_none(),
                    "backend exited during startup"
                );
                if let Ok(response) = client.get(format!("{}/health", self.endpoint)).send().await
                    && response.status().is_success()
                {
                    return Ok::<_, anyhow::Error>(());
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .context("backend readiness deadline")?
    }

    async fn kill(&mut self) -> Result<()> {
        self.child.start_kill()?;
        tokio::time::timeout(Duration::from_secs(5), self.child.wait())
            .await
            .context("backend termination deadline")??;
        Ok(())
    }
}

struct AbortTask<T>(JoinHandle<T>);

impl<T> Drop for AbortTask<T> {
    fn drop(&mut self) {
        self.0.abort();
    }
}

async fn seed(pool: &PgPool) -> Result<(Uuid, String)> {
    let user_id = Uuid::new_v4();
    let document_id = Uuid::new_v4();
    let section_id = Uuid::new_v4();
    let token = Uuid::new_v4().to_string();
    sqlx::query("insert into users (id, username, display_name, email, password_hash, role) values ($1, 'recovery-author', 'Recovery Author', 'recovery@example.test', 'unused', 'normal')")
        .bind(user_id).execute(pool).await?;
    sqlx::query("insert into user_sessions (id, token, user_id, expires_at) values ($1, $2, $3, now() + interval '1 day')")
        .bind(Uuid::new_v4()).bind(&token).bind(user_id).execute(pool).await?;
    sqlx::query("insert into documents (id, slug, title, visibility, markdown_policy, created_by) values ($1, 'recovery', 'Recovery', 'public', '{}'::jsonb, $2)")
        .bind(document_id).bind(user_id).execute(pool).await?;
    sqlx::query("insert into sections (id, document_id, parent_id, title, position, path) values ($1, $2, null, 'Recovery', 0, $3)")
        .bind(section_id).bind(document_id).bind(section_id.to_string()).execute(pool).await?;
    Ok((section_id, token))
}

async fn exercise_restart(pool: &PgPool, database_url: &str, files: &TestFiles) -> Result<()> {
    sqlx::migrate!("../../migrations").run(pool).await?;
    let (section_id, token) = seed(pool).await?;
    let client = Client::builder().timeout(Duration::from_secs(2)).build()?;

    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let embedding_url = format!("http://{}", listener.local_addr()?);
    let (release, waiting) = tokio::sync::watch::channel(false);
    let (arrived, mut requests) = mpsc::unbounded_channel();
    let embedding_app = Router::new().route(
        "/embed",
        post(move || {
            let mut waiting = waiting.clone();
            let arrived = arrived.clone();
            async move {
                let _ = arrived.send(());
                let _ = waiting.wait_for(|ready| *ready).await;
                Json(json!({"model_name": "recovery-test", "embedding": [1.0, 0.0]}))
            }
        }),
    );
    let _embedding_server = AbortTask(tokio::spawn(async move {
        axum::serve(listener, embedding_app).await
    }));

    let mut backend = Backend::start(
        database_url,
        &embedding_url,
        files.0.join("before-crash.log"),
    )
    .await?;
    backend.ready(&client).await?;
    let publish_client = Client::builder().timeout(Duration::from_secs(20)).build()?;
    let publish_url = format!("{}/api/sections/{section_id}/publish", backend.endpoint);
    let mut publisher = AbortTask(tokio::spawn(async move {
        publish_client.post(publish_url)
            .header("x-rimbun-session", token)
            .json(&json!({"base_submission_id": null, "markdown_content": "Recovered after a real backend crash."}))
            .send().await
    }));

    // Both the immediate rebuild and the queue worker reach HTTP after commit.
    // Keeping them blocked makes the crash window deterministic.
    tokio::time::timeout(Duration::from_secs(4), async {
        requests.recv().await.context("first embedding request")?;
        requests.recv().await.context("worker embedding request")?;
        Ok::<_, anyhow::Error>(())
    })
    .await
    .context("both rebuilds reach the embedding barrier")??;

    let submission_id =
        sqlx::query_scalar::<_, Uuid>("select id from submissions where section_id = $1")
            .bind(section_id)
            .fetch_one(pool)
            .await?;
    let lease = sqlx::query_as::<_, (Uuid, bool)>(
        "select lease_token, lease_until > now() from projection_jobs where section_id = $1",
    )
    .bind(section_id)
    .fetch_one(pool)
    .await?;
    ensure!(lease.1, "worker must have an active lease before the crash");
    let projection_count = sqlx::query_scalar::<_, i64>(
        "select count(*) from section_projection_items where section_id = $1",
    )
    .bind(section_id)
    .fetch_one(pool)
    .await?;
    ensure!(
        projection_count == 0,
        "projection must be absent before crash"
    );

    backend.kill().await?;
    let publication = tokio::time::timeout(Duration::from_secs(3), &mut publisher.0)
        .await
        .context("interrupted publication deadline")??;
    ensure!(
        publication.is_err(),
        "hard termination must interrupt the HTTP response"
    );
    let remaining_lease = sqlx::query_scalar::<_, Uuid>(
        "select lease_token from projection_jobs where section_id = $1",
    )
    .bind(section_id)
    .fetch_one(pool)
    .await?;
    ensure!(
        remaining_lease == lease.0,
        "crashed worker must leave its durable lease"
    );

    release.send(true)?;
    let mut restarted = Backend::start(
        database_url,
        &embedding_url,
        files.0.join("after-restart.log"),
    )
    .await?;
    restarted.ready(&client).await?;

    // Use the actual production lease expiry; do not edit queue timestamps.
    tokio::time::timeout(Duration::from_secs(80), async {
        loop {
            ensure!(restarted.child.try_wait()?.is_none(), "restarted backend exited");
            let repaired = sqlx::query_scalar::<_, bool>(
                "select exists(select 1 from section_projection_items where section_id = $1 and submission_id = $2 and role = 'main') and not exists(select 1 from projection_jobs where section_id = $1)",
            ).bind(section_id).bind(submission_id).fetch_one(pool).await?;
            if repaired { return Ok::<_, anyhow::Error>(()); }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }).await.context("backend must reclaim the expired lease and drain the queue")??;

    // This is the endpoint consumed by the document's reading view.
    let read: Value = client
        .get(format!(
            "{}/api/sections/{section_id}/compare",
            restarted.endpoint
        ))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    ensure!(
        read["main_submission"]["submission_id"] == submission_id.to_string(),
        "reader must show the recovered submission"
    );
    ensure!(
        read["main_submission"]["markdown_content"] == "Recovered after a real backend crash.",
        "reader must preserve the published text"
    );
    let submissions =
        sqlx::query_scalar::<_, i64>("select count(*) from submissions where section_id = $1")
            .bind(section_id)
            .fetch_one(pool)
            .await?;
    ensure!(
        submissions == 1,
        "recovery must not republish the contribution"
    );
    restarted.kill().await?;
    Ok(())
}

#[tokio::test]
async fn backend_restart_recovers_committed_publication_and_expired_worker_lease() -> Result<()> {
    let database_url = match std::env::var("TEST_DATABASE_URL") {
        Ok(url) => url,
        Err(error) if std::env::var_os("RIMBUN_REQUIRE_TEST_DATABASE").is_some() => {
            return Err(error).context("TEST_DATABASE_URL is required");
        }
        Err(_) => {
            eprintln!("Skipping process recovery test: TEST_DATABASE_URL is not set");
            return Ok(());
        }
    };
    let admin = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await?;
    let schema = format!("recovery_{}", Uuid::new_v4().simple());
    sqlx::query(&format!("create schema {schema}"))
        .execute(&admin)
        .await?;
    let mut isolated_url = reqwest::Url::parse(&database_url)?;
    isolated_url
        .query_pairs_mut()
        .append_pair("options[search_path]", &schema);
    let files = TestFiles::new()?;
    let result = async {
        let pool = PgPoolOptions::new()
            .max_connections(3)
            .connect(isolated_url.as_str())
            .await?;
        let result = tokio::time::timeout(
            Duration::from_secs(110),
            exercise_restart(&pool, isolated_url.as_str(), &files),
        )
        .await
        .context("process recovery test deadline");
        pool.close().await;
        result?
    }
    .await;
    let cleanup = sqlx::query(&format!("drop schema {schema} cascade"))
        .execute(&admin)
        .await;
    admin.close().await;
    result.with_context(|| files.diagnostics())?;
    cleanup.context("remove isolated recovery schema")?;
    Ok(())
}
