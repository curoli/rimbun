use std::{collections::BTreeMap, time::Duration};

use serde::de::DeserializeOwned;
use sqlx::{ConnectOptions, Connection, PgConnection, postgres::PgConnectOptions};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const QUERY_TIMEOUT: Duration = Duration::from_secs(3);

// Never expose driver errors: connection URLs and server messages may contain secrets.
#[derive(Debug, PartialEq, Eq)]
pub enum StatusError {
    MissingUrl,
    InvalidUrl,
    ConnectFailed,
    ConnectTimeout,
    QueryFailed,
    QueryTimeout,
    RuntimeUnavailable,
}

impl StatusError {
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

pub fn read_json<T: DeserializeOwned>(url: &str, query: &str) -> Result<T, StatusError> {
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
