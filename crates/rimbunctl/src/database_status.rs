use std::{
    collections::BTreeMap,
    ffi::OsString,
    io::{Read, Write},
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sqlx::{ConnectOptions, Connection, PgConnection, postgres::PgConnectOptions};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const QUERY_TIMEOUT: Duration = Duration::from_secs(3);
pub const WORKER_ARGUMENT: &str = "--internal-database-status";
pub const MIGRATION_STATUS_QUERY: &str = "SELECT coalesce(json_agg(json_build_object('version', version, 'success', success) ORDER BY version), '[]'::json) FROM _sqlx_migrations";

// Never expose driver errors: connection URLs and server messages may contain secrets.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StatusError {
    MissingUrl,
    InvalidUrl,
    ConnectFailed,
    ConnectTimeout,
    QueryFailed,
    QueryTimeout,
    RuntimeUnavailable,
}

pub fn read_profile_json<T: DeserializeOwned>(
    executable: &Path,
    inherited_env: BTreeMap<OsString, OsString>,
    profile_env: &BTreeMap<String, String>,
    query: &str,
) -> Result<T, StatusError> {
    // Keep stdin below PIPE_BUF so writing a fixed probe cannot block on startup.
    if query.len() > 4096 {
        return Err(StatusError::QueryFailed);
    }
    let mut environment = inherited_env;
    environment.extend(
        profile_env
            .iter()
            .map(|(key, value)| (key.into(), value.into())),
    );
    let mut child = Command::new(executable)
        .arg(WORKER_ARGUMENT)
        .env_clear()
        .envs(environment)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| StatusError::RuntimeUnavailable)?;
    // Only the fixed SQL query goes through stdin; credentials never enter argv.
    if child
        .stdin
        .take()
        .unwrap()
        .write_all(query.as_bytes())
        .is_err()
    {
        terminate_worker(child);
        return Err(StatusError::RuntimeUnavailable);
    }
    let stdout = child.stdout.take().unwrap();
    let reader = std::thread::spawn(move || {
        let mut output = Vec::new();
        stdout
            .take(1_048_576)
            .read_to_end(&mut output)
            .map(|_| output)
    });
    let deadline = Instant::now() + Duration::from_secs(6);
    let completed = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.success(),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            _ => {
                // A stalled filesystem must not turn kill/reaping or pipe cleanup
                // into another unbounded wait on the caller's deadline path.
                terminate_worker(child);
                return Err(StatusError::QueryTimeout);
            }
        }
    };
    let output = reader
        .join()
        .ok()
        .and_then(Result::ok)
        .ok_or(StatusError::RuntimeUnavailable)?;
    if !completed {
        return Err(StatusError::RuntimeUnavailable);
    }
    let result: Result<serde_json::Value, StatusError> =
        serde_json::from_slice(&output).map_err(|_| StatusError::RuntimeUnavailable)?;
    result.and_then(|value| serde_json::from_value(value).map_err(|_| StatusError::QueryFailed))
}

fn terminate_worker(mut child: std::process::Child) {
    let _ = child.kill();
    std::thread::spawn(move || {
        let _ = child.wait();
    });
}

pub fn run_worker() -> std::process::ExitCode {
    let result = (|| {
        let mut query = String::new();
        std::io::stdin()
            .take(4096)
            .read_to_string(&mut query)
            .map_err(|_| StatusError::QueryFailed)?;
        let url = database_url(&BTreeMap::new(), std::env::var("DATABASE_URL").ok())?;
        read_json::<serde_json::Value>(&url, &query)
    })();
    match serde_json::to_writer(std::io::stdout(), &result) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(_) => std::process::ExitCode::FAILURE,
    }
}

impl StatusError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::MissingUrl => "database_url_missing",
            Self::InvalidUrl => "database_url_invalid",
            Self::ConnectFailed => "database_connect_failed",
            Self::ConnectTimeout => "database_connect_timeout",
            Self::QueryFailed => "database_query_failed",
            Self::QueryTimeout => "database_query_timeout",
            Self::RuntimeUnavailable => "database_probe_unavailable",
        }
    }

    pub fn message(&self) -> &'static str {
        match self {
            Self::MissingUrl => "unknown (DATABASE_URL missing)",
            Self::InvalidUrl => "unknown (invalid DATABASE_URL)",
            Self::ConnectFailed => "unknown (database connection failed)",
            Self::ConnectTimeout => "unknown (database connection timed out)",
            Self::QueryFailed => "unknown (database query failed)",
            Self::QueryTimeout => "unknown (database query timed out)",
            Self::RuntimeUnavailable => "unknown (database runtime unavailable)",
        }
    }
}

pub fn database_url(
    profile_env: &BTreeMap<String, String>,
    inherited_url: Option<String>,
) -> Result<String, StatusError> {
    // Match service environment precedence, with no fallback to a different database.
    profile_env
        .get("DATABASE_URL")
        .cloned()
        .or(inherited_url)
        .filter(|url| !url.trim().is_empty())
        .ok_or(StatusError::MissingUrl)
}

fn read_json<T: DeserializeOwned>(url: &str, query: &str) -> Result<T, StatusError> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| StatusError::RuntimeUnavailable)?;
    let result = runtime.block_on(read_json_async(url, query));
    // A timed-out DNS lookup must not make runtime shutdown wait indefinitely.
    runtime.shutdown_background();
    result.and_then(|value| serde_json::from_value(value).map_err(|_| StatusError::QueryFailed))
}

async fn read_json_async(url: &str, query: &str) -> Result<serde_json::Value, StatusError> {
    let options = url
        .parse::<PgConnectOptions>()
        .map_err(|_| StatusError::InvalidUrl)?
        .disable_statement_logging();
    let mut connection =
        tokio::time::timeout(CONNECT_TIMEOUT, PgConnection::connect_with(&options))
            .await
            .map_err(|_| StatusError::ConnectTimeout)?
            .map_err(|_| StatusError::ConnectFailed)?;
    tokio::time::timeout(QUERY_TIMEOUT, async {
        let mut tx = connection.begin().await?;
        sqlx::raw_sql("SET TRANSACTION READ ONLY; SET LOCAL statement_timeout = '3s'")
            .execute(&mut *tx)
            .await?;
        let value: sqlx::types::Json<serde_json::Value> =
            sqlx::query_scalar(query).fetch_one(&mut *tx).await?;
        tx.rollback().await?;
        Ok::<_, sqlx::Error>(value.0)
    })
    .await
    .map_err(|_| StatusError::QueryTimeout)?
    .map_err(|error| {
        if error
            .as_database_error()
            .and_then(|error| error.code())
            .as_deref()
            == Some("57014")
        {
            StatusError::QueryTimeout
        } else {
            StatusError::QueryFailed
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn database_url_matches_service_environment_precedence() {
        let profile = BTreeMap::from([("DATABASE_URL".to_owned(), "profile-url".to_owned())]);
        assert_eq!(
            database_url(&profile, Some("inherited-url".to_owned())),
            Ok("profile-url".to_owned())
        );
        assert_eq!(
            database_url(&BTreeMap::new(), Some("inherited-url".to_owned())),
            Ok("inherited-url".to_owned())
        );
        assert_eq!(
            database_url(&BTreeMap::new(), None),
            Err(StatusError::MissingUrl)
        );
        let empty = BTreeMap::from([("DATABASE_URL".to_owned(), String::new())]);
        assert_eq!(
            database_url(&empty, Some("inherited-url".to_owned())),
            Err(StatusError::MissingUrl)
        );
    }

    #[test]
    fn invalid_connection_diagnostics_do_not_disclose_secrets() {
        let error = read_json::<serde_json::Value>(
            "postgres://alice:super-secret@localhost:not-a-port/database",
            "select '{}'::json",
        )
        .unwrap_err();
        assert_eq!(error, StatusError::InvalidUrl);
        assert!(!error.message().contains("super-secret"));
        assert!(!format!("{error:?}").contains("alice"));
    }

    #[test]
    fn stalled_postgres_handshake_has_a_bounded_deadline() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        listener.set_nonblocking(true).unwrap();
        let (release, waiting) = std::sync::mpsc::channel();
        let server = std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + Duration::from_secs(6);
            while std::time::Instant::now() < deadline {
                match listener.accept() {
                    Ok((_stream, _)) => {
                        let _ = waiting.recv_timeout(Duration::from_secs(6));
                        return;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        if waiting.try_recv().is_ok() {
                            return;
                        }
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Err(error) => panic!("mock PostgreSQL listener failed: {error}"),
                }
            }
        });
        let started = std::time::Instant::now();
        let result = read_json::<serde_json::Value>(
            &format!("postgres://user:secret@127.0.0.1:{port}/test?sslmode=disable"),
            "select '{}'::json",
        );
        let _ = release.send(());
        server.join().unwrap();
        assert_eq!(result.unwrap_err(), StatusError::ConnectTimeout);
        assert!(started.elapsed() < Duration::from_secs(5));
    }
}
