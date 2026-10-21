use std::{collections::BTreeMap, ffi::OsString, path::Path, time::Duration};

use anyhow::Result;
use sqlx::Connection;

// Exercise the production worker launcher using Cargo's actual CLI executable.
#[allow(dead_code)]
#[path = "../src/database_status.rs"]
mod database_status;
#[allow(dead_code)]
#[path = "../src/projection_status.rs"]
mod projection_status;

#[test]
fn worker_database_queries_cover_migrations_queue_and_read_only_failures() -> Result<()> {
    use sqlx::Connection;

    let database_url = match std::env::var("TEST_DATABASE_URL") {
        Ok(url) => url,
        Err(error) if std::env::var_os("RIMBUN_REQUIRE_TEST_DATABASE").is_some() => {
            return Err(error.into());
        }
        Err(_) => {
            eprintln!("Skipping direct database status test: TEST_DATABASE_URL is not set");
            return Ok(());
        }
    };
    struct TestDatabase {
        runtime: tokio::runtime::Runtime,
        admin: sqlx::PgConnection,
        schema: String,
    }
    impl Drop for TestDatabase {
        fn drop(&mut self) {
            self.runtime.block_on(async {
                let command = format!("drop schema {} cascade", self.schema);
                let cleanup = sqlx::query(&command);
                let _ =
                    tokio::time::timeout(Duration::from_secs(5), cleanup.execute(&mut self.admin))
                        .await;
            });
        }
    }
    let runtime = tokio::runtime::Runtime::new()?;
    let mut admin = runtime.block_on(sqlx::PgConnection::connect(&database_url))?;
    let schema = format!("status_{}", uuid::Uuid::new_v4().simple());
    runtime.block_on(sqlx::query(&format!("create schema {schema}")).execute(&mut admin))?;
    let fixture = TestDatabase {
        runtime,
        admin,
        schema,
    };
    let mut url = reqwest::Url::parse(&database_url)?;
    url.query_pairs_mut()
        .append_pair("options[search_path]", &fixture.schema);
    fixture.runtime.block_on(async {
        let mut connection = sqlx::PgConnection::connect(url.as_str()).await?;
        sqlx::migrate!("../../migrations")
            .run(&mut connection)
            .await?;
        Ok::<_, anyhow::Error>(())
    })?;

    let executable = Path::new(env!("CARGO_BIN_EXE_rimbunctl"));
    let read = |url: &str, query: &str| {
        let profile = BTreeMap::from([("DATABASE_URL".to_owned(), url.to_owned())]);
        database_status::read_profile_json::<serde_json::Value>(
            executable,
            std::env::vars_os().collect(),
            &profile,
            query,
        )
    };
    let migrations = read(url.as_str(), database_status::MIGRATION_STATUS_QUERY).unwrap();
    assert_eq!(
        migrations.as_array().unwrap().len(),
        sqlx::migrate!("../../migrations").iter().count()
    );
    assert!(
        migrations
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["success"] == true)
    );
    let queue: projection_status::QueueStatus =
        serde_json::from_value(read(url.as_str(), projection_status::QUERY).unwrap())?;
    assert!(queue.healthy(projection_status::HealthThresholds::default()));
    assert!(
        queue
            .summary(projection_status::HealthThresholds::default())
            .contains("0 pending")
    );
    assert_eq!(
        read(
            url.as_str(),
            "select to_json(current_setting('transaction_read_only'))"
        )
        .unwrap(),
        "on"
    );

    let mut denied_url = url.clone();
    denied_url
        .set_username(&format!("credential_{}", uuid::Uuid::new_v4().simple()))
        .unwrap();
    denied_url
        .set_password(Some("password_must_not_appear"))
        .unwrap();
    assert_eq!(
        read(denied_url.as_str(), "select '{}'::json").unwrap_err(),
        database_status::StatusError::ConnectFailed
    );

    fixture.runtime.block_on(async {
        let mut connection = sqlx::PgConnection::connect(url.as_str()).await?;
        sqlx::query("insert into projection_jobs(section_id, queued_at) values ($1, now() - interval '5 minutes')")
            .bind(uuid::Uuid::new_v4()).execute(&mut connection).await?;
        Ok::<_, anyhow::Error>(())
    })?;
    let queue: projection_status::QueueStatus =
        serde_json::from_value(read(url.as_str(), projection_status::QUERY).unwrap())?;
    assert!(!queue.healthy(projection_status::HealthThresholds::default()));
    assert!(
        queue
            .summary(projection_status::HealthThresholds::default())
            .contains("1 pending")
    );
    assert_eq!(
        read(
            url.as_str(),
            "create table status_must_not_write(id integer)"
        )
        .unwrap_err(),
        database_status::StatusError::QueryFailed
    );
    assert_eq!(
        read(
            url.as_str(),
            "select to_json(to_regclass('status_must_not_write') is not null)"
        )
        .unwrap(),
        false
    );
    let started = std::time::Instant::now();
    assert_eq!(
        read(url.as_str(), "select pg_sleep(10), '{}'::json").unwrap_err(),
        database_status::StatusError::QueryTimeout
    );
    assert!(started.elapsed() < Duration::from_secs(6));
    assert_eq!(
        read(
            url.as_str(),
            "select * from secret_password_must_not_appear"
        )
        .unwrap_err()
        .message(),
        "unknown (database query failed)"
    );
    fixture.runtime.block_on(async {
        let mut connection = sqlx::PgConnection::connect(url.as_str()).await?;
        sqlx::raw_sql("drop table projection_jobs; drop table _sqlx_migrations")
            .execute(&mut connection)
            .await?;
        Ok::<_, anyhow::Error>(())
    })?;
    assert_eq!(
        read(url.as_str(), projection_status::QUERY).unwrap_err(),
        database_status::StatusError::QueryFailed
    );
    assert_eq!(
        read(url.as_str(), database_status::MIGRATION_STATUS_QUERY).unwrap_err(),
        database_status::StatusError::QueryFailed
    );
    assert_eq!(
        read(
            "postgres://alice:secret@localhost:invalid/database",
            projection_status::QUERY
        )
        .unwrap_err(),
        database_status::StatusError::InvalidUrl
    );
    Ok(())
}

#[test]
fn blocked_profile_password_file_is_killed_by_parent_deadline() -> Result<()> {
    blocked_password_file_is_killed(false)
}

#[test]
fn inherited_password_file_is_killed_with_url_only_profile() -> Result<()> {
    blocked_password_file_is_killed(true)
}

fn blocked_password_file_is_killed(inherited_file: bool) -> Result<()> {
    let path = std::env::temp_dir().join(format!("rimbun-pgpass-{}", uuid::Uuid::new_v4()));
    struct Fifo(std::path::PathBuf);
    impl Drop for Fifo {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    nix::unistd::mkfifo(
        &path,
        nix::sys::stat::Mode::S_IRUSR | nix::sys::stat::Mode::S_IWUSR,
    )?;
    let _cleanup = Fifo(path.clone());
    let mut inherited = BTreeMap::from([(
        "DATABASE_URL".into(),
        "postgres://probe@127.0.0.1/test?sslmode=disable".into(),
    )]);
    let mut profile = BTreeMap::new();
    if inherited_file {
        inherited.insert("PGPASSFILE".into(), path.as_os_str().to_owned());
        profile.insert(
            "DATABASE_URL".to_owned(),
            "postgres://probe@127.0.0.1/test?sslmode=disable".to_owned(),
        );
    } else {
        profile.insert("PGPASSFILE".to_owned(), path.to_str().unwrap().to_owned());
    }
    let started = std::time::Instant::now();
    let result = database_status::read_profile_json::<serde_json::Value>(
        Path::new(env!("CARGO_BIN_EXE_rimbunctl")),
        inherited,
        &profile,
        "select '{}'::json",
    );
    assert_eq!(
        result.unwrap_err(),
        database_status::StatusError::QueryTimeout
    );
    assert!(started.elapsed() < Duration::from_secs(9));
    Ok(())
}

#[test]
fn profile_password_and_tls_environment_override_inherited_values() -> Result<()> {
    let parent_password = std::env::var_os("PGPASSWORD");
    let parent_ssl_mode = std::env::var_os("PGSSLMODE");
    let database_url = match std::env::var("TEST_DATABASE_URL") {
        Ok(url) => url,
        Err(error) if std::env::var_os("RIMBUN_REQUIRE_TEST_DATABASE").is_some() => {
            return Err(error.into());
        }
        Err(_) => {
            eprintln!("Skipping PostgreSQL environment regression: TEST_DATABASE_URL is not set");
            return Ok(());
        }
    };
    struct Role {
        runtime: tokio::runtime::Runtime,
        connection: sqlx::PgConnection,
        name: String,
    }
    impl Drop for Role {
        fn drop(&mut self) {
            self.runtime.block_on(async {
                let query = format!("drop role {}", self.name);
                let _ = tokio::time::timeout(
                    Duration::from_secs(5),
                    sqlx::query(&query).execute(&mut self.connection),
                )
                .await;
            });
        }
    }
    let runtime = tokio::runtime::Runtime::new()?;
    let mut connection = runtime.block_on(sqlx::PgConnection::connect(&database_url))?;
    let ssl: String =
        runtime.block_on(sqlx::query_scalar("show ssl").fetch_one(&mut connection))?;
    assert_eq!(
        ssl, "off",
        "regression requires the plaintext PostgreSQL test service"
    );
    let name = format!("status_env_{}", uuid::Uuid::new_v4().simple());
    let password = uuid::Uuid::new_v4().simple().to_string();
    runtime.block_on(
        sqlx::query(&format!("create role {name} login password '{password}'"))
            .execute(&mut connection),
    )?;
    let role = Role {
        runtime,
        connection,
        name,
    };
    let mut url = reqwest::Url::parse(&database_url)?;
    url.set_username(&role.name).unwrap();
    url.set_password(None).unwrap();
    let pairs: Vec<_> = url
        .query_pairs()
        .filter(|(key, _)| !matches!(key.as_ref(), "sslmode" | "ssl-mode" | "password" | "user"))
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();
    url.set_query(None);
    url.query_pairs_mut().extend_pairs(pairs);
    let mut inherited: BTreeMap<OsString, OsString> = std::env::vars_os()
        .filter(|(key, _)| !key.to_string_lossy().starts_with("PG"))
        .collect();
    inherited.insert("DATABASE_URL".into(), url.as_str().into());
    inherited.insert("PGPASSWORD".into(), "wrong-inherited-password".into());
    inherited.insert("PGSSLMODE".into(), "disable".into());
    inherited.insert("PGAPPNAME".into(), "inherited-probe".into());
    inherited.insert("PGOPTIONS".into(), "-c work_mem=6MB".into());
    let mut profile = BTreeMap::from([
        ("PGPASSWORD".to_owned(), password.clone()),
        ("PGSSLMODE".to_owned(), "disable".to_owned()),
        ("PGAPPNAME".to_owned(), "profile-status-probe".to_owned()),
        ("PGOPTIONS".to_owned(), "-c work_mem=5MB".to_owned()),
    ]);
    let executable = Path::new(env!("CARGO_BIN_EXE_rimbunctl"));
    let probe =
        |profile: &BTreeMap<String, String>, inherited: &BTreeMap<OsString, OsString>, query| {
            database_status::read_profile_json::<serde_json::Value>(
                executable,
                inherited.clone(),
                profile,
                query,
            )
        };
    assert_eq!(
        probe(&profile, &inherited, "select to_json(current_user)").unwrap(),
        role.name
    );
    assert_eq!(
        probe(
            &profile,
            &inherited,
            "select to_json(current_setting('application_name'))"
        )
        .unwrap(),
        "profile-status-probe"
    );
    assert_eq!(
        probe(
            &profile,
            &inherited,
            "select to_json(current_setting('work_mem'))"
        )
        .unwrap(),
        "5MB"
    );

    inherited.insert("PGPASSWORD".into(), password.clone().into());
    profile.insert("PGPASSWORD".to_owned(), "wrong-profile-password".to_owned());
    assert_eq!(
        probe(&profile, &inherited, "select '{}'::json").unwrap_err(),
        database_status::StatusError::ConnectFailed
    );
    profile.remove("PGPASSWORD");
    assert!(probe(&profile, &inherited, "select '{}'::json").is_ok());
    profile.insert("PGSSLMODE".to_owned(), "require".to_owned());
    assert_eq!(
        probe(&profile, &inherited, "select '{}'::json").unwrap_err(),
        database_status::StatusError::ConnectFailed
    );
    profile.insert("PGSSLMODE".to_owned(), "disable".to_owned());
    inherited.insert("PGSSLMODE".into(), "require".into());
    assert!(probe(&profile, &inherited, "select '{}'::json").is_ok());

    // Explicit URL settings retain SQLx's precedence over all PG* variables.
    url.set_password(Some(&password)).unwrap();
    url.query_pairs_mut().append_pair("sslmode", "disable");
    profile.insert("DATABASE_URL".to_owned(), url.to_string());
    profile.insert("PGPASSWORD".to_owned(), "wrong-profile-password".to_owned());
    profile.insert("PGSSLMODE".to_owned(), "require".to_owned());
    assert!(probe(&profile, &inherited, "select '{}'::json").is_ok());
    assert_eq!(std::env::var_os("PGPASSWORD"), parent_password);
    assert_eq!(std::env::var_os("PGSSLMODE"), parent_ssl_mode);
    Ok(())
}
