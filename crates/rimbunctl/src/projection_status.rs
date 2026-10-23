#[cfg(test)]
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::num::NonZeroU64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct HealthThresholds {
    pub projection_max_age_seconds: NonZeroU64,
    pub projection_max_failed_attempts: NonZeroU64,
}

impl Default for HealthThresholds {
    fn default() -> Self {
        Self {
            projection_max_age_seconds: NonZeroU64::new(300).unwrap(),
            projection_max_failed_attempts: NonZeroU64::new(3).unwrap(),
        }
    }
}

// A single statement keeps the counters and diagnostic from one database snapshot.
pub const QUERY: &str = r#"
select json_build_object(
    'pending', count(*),
    'running', count(*) filter (where lease_until > now()),
    'failed', count(*) filter (where last_error is not null),
    'expired_leases', count(*) filter (where lease_until <= now()),
    'oldest_seconds', coalesce(floor(greatest(0, extract(epoch from now() - min(queued_at)))), 0),
    'max_attempts', coalesce(max(attempts), 0),
    'failure', (select json_build_object('section_id', section_id, 'attempts', attempts,
                    'message', left(last_error, 600))
                from projection_jobs where last_error is not null
                order by attempts desc, updated_at desc, section_id limit 1)
) from projection_jobs
"#;

#[derive(Debug, Deserialize, Serialize)]
pub struct QueueStatus {
    pending: u64,
    running: u64,
    failed: u64,
    expired_leases: u64,
    oldest_seconds: u64,
    max_attempts: u64,
    failure: Option<JobFailure>,
}

#[derive(Debug, Deserialize, Serialize)]
struct JobFailure {
    section_id: String,
    attempts: u64,
    #[serde(skip_serializing)]
    message: String,
}

impl QueueStatus {
    pub fn reason_codes(&self, thresholds: HealthThresholds) -> Vec<&'static str> {
        let mut reasons = Vec::new();
        if self.oldest_seconds >= thresholds.projection_max_age_seconds.get() {
            reasons.push("projection_age_exceeded");
        }
        if self.max_attempts >= thresholds.projection_max_failed_attempts.get() {
            reasons.push("projection_retry_limit_exceeded");
        }
        reasons
    }
    #[cfg(test)]
    pub fn parse(raw: &[u8]) -> Result<Self> {
        Ok(serde_json::from_slice(raw)?)
    }

    pub fn healthy(&self, thresholds: HealthThresholds) -> bool {
        self.oldest_seconds < thresholds.projection_max_age_seconds.get()
            && self.max_attempts < thresholds.projection_max_failed_attempts.get()
    }

    pub fn summary(&self, thresholds: HealthThresholds) -> String {
        let oldest = if self.pending == 0 {
            "none".to_owned()
        } else {
            format!("{}s", self.oldest_seconds)
        };
        let mut summary = format!(
            "{} pending, {} running, {} with errors, {} expired leases; oldest: {oldest}; max failed attempts: {}",
            self.pending, self.running, self.failed, self.expired_leases, self.max_attempts
        );
        if !self.healthy(thresholds) {
            summary.push_str(" [degraded]");
        }
        if let Some(failure) = &self.failure {
            // Do not let multiline errors or terminal control sequences corrupt status output.
            let message: String = failure
                .message
                .chars()
                .flat_map(char::escape_debug)
                .collect();
            summary.push_str(&format!(
                "\n                 failing section: {} ({} failed attempts); last error: {message}",
                failure.section_id, failure.attempts
            ));
        }
        summary
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(age: u64, attempts: u64) -> QueueStatus {
        QueueStatus {
            pending: 1,
            running: 0,
            failed: u64::from(attempts > 0),
            expired_leases: 0,
            oldest_seconds: age,
            max_attempts: attempts,
            failure: None,
        }
    }

    #[test]
    fn queue_health_has_explicit_age_and_retry_boundaries() {
        assert!(status(299, 2).healthy(HealthThresholds::default()));
        assert!(!status(300, 0).healthy(HealthThresholds::default()));
        assert!(!status(0, 3).healthy(HealthThresholds::default()));
        assert!(status(0, 0).healthy(HealthThresholds::default()));
    }

    #[test]
    fn custom_thresholds_use_inclusive_degradation_boundaries() {
        let thresholds = HealthThresholds {
            projection_max_age_seconds: NonZeroU64::new(60).unwrap(),
            projection_max_failed_attempts: NonZeroU64::new(2).unwrap(),
        };
        assert!(status(59, 1).healthy(thresholds));
        assert!(!status(60, 0).healthy(thresholds));
        assert!(!status(0, 2).healthy(thresholds));
        assert!(status(60, 0).summary(thresholds).contains("[degraded]"));
        assert!(!status(59, 1).summary(thresholds).contains("[degraded]"));
    }

    #[test]
    fn queue_parser_rejects_missing_or_invalid_counters() {
        assert!(QueueStatus::parse(b"{}").is_err());
        assert!(QueueStatus::parse(b"not json").is_err());
        assert!(QueueStatus::parse(br#"{"pending":-1}"#).is_err());
    }

    #[test]
    fn queue_summary_escapes_error_text() {
        let mut report = status(301, 3);
        report.failure = Some(JobFailure {
            section_id: "test-section".to_owned(),
            attempts: 3,
            message: "connection failed\n\u{1b}[31m".to_owned(),
        });
        let summary = report.summary(HealthThresholds::default());
        assert!(summary.contains("[degraded]"));
        assert!(summary.contains("connection failed\\n\\u{1b}[31m"));
        assert!(!summary.contains('\u{1b}'));
    }

    #[tokio::test]
    async fn queue_query_reports_empty_running_failed_and_stale_jobs() -> Result<()> {
        let database_url = match std::env::var("TEST_DATABASE_URL") {
            Ok(url) => url,
            Err(error) if std::env::var_os("RIMBUN_REQUIRE_TEST_DATABASE").is_some() => {
                return Err(error.into());
            }
            Err(_) => {
                eprintln!("Skipping queue query test: TEST_DATABASE_URL is not set");
                return Ok(());
            }
        };
        let mut connection = sqlx::postgres::PgConnection::connect(&database_url).await?;
        use sqlx::Connection;
        let mut tx = connection.begin().await?;
        // A temporary table shadows the real queue without touching project data.
        sqlx::query("create temporary table projection_jobs (section_id uuid, attempts integer not null default 0, lease_until timestamptz, last_error text, updated_at timestamptz not null default now()) on commit drop")
            .execute(&mut *tx).await?;
        sqlx::query("insert into projection_jobs(section_id, updated_at) values ('00000000-0000-0000-0000-000000000003', now() - interval '600 seconds')")
            .execute(&mut *tx).await?;
        sqlx::raw_sql(include_str!(
            "../../../migrations/0020_projection_queue_age.sql"
        ))
        .execute(&mut *tx)
        .await?;
        let backfilled: serde_json::Value = sqlx::query_scalar(QUERY).fetch_one(&mut *tx).await?;
        let report = QueueStatus::parse(&serde_json::to_vec(&backfilled)?)?;
        assert_eq!(report.oldest_seconds, 600);
        assert!(!report.healthy(HealthThresholds::default()));
        let preserved: bool =
            sqlx::query_scalar("select queued_at = updated_at from projection_jobs")
                .fetch_one(&mut *tx)
                .await?;
        assert!(preserved);
        sqlx::query("delete from projection_jobs")
            .execute(&mut *tx)
            .await?;
        let empty: serde_json::Value = sqlx::query_scalar(QUERY).fetch_one(&mut *tx).await?;
        let report = QueueStatus::parse(&serde_json::to_vec(&empty)?)?;
        assert!(report.healthy(HealthThresholds::default()));
        assert_eq!(report.pending, 0);
        assert!(
            report
                .summary(HealthThresholds::default())
                .contains("oldest: none")
        );

        sqlx::query("insert into projection_jobs(section_id, lease_until) values ('00000000-0000-0000-0000-000000000001', now() + interval '60 seconds')")
            .execute(&mut *tx).await?;
        let running: serde_json::Value = sqlx::query_scalar(QUERY).fetch_one(&mut *tx).await?;
        let report = QueueStatus::parse(&serde_json::to_vec(&running)?)?;
        assert!(report.healthy(HealthThresholds::default()));
        assert_eq!(report.running, 1);

        sqlx::query("insert into projection_jobs(section_id, attempts, last_error, lease_until, queued_at) values ('00000000-0000-0000-0000-000000000002', 3, 'embedding unavailable', now() - interval '1 second', now() - interval '300 seconds')")
            .execute(&mut *tx).await?;
        let failed: serde_json::Value = sqlx::query_scalar(QUERY).fetch_one(&mut *tx).await?;
        let report = QueueStatus::parse(&serde_json::to_vec(&failed)?)?;
        assert!(!report.healthy(HealthThresholds::default()));
        assert_eq!(report.pending, 2);
        assert_eq!(report.running, 1);
        assert_eq!(report.failed, 1);
        assert_eq!(report.expired_leases, 1);
        assert_eq!(report.oldest_seconds, 300);
        assert_eq!(report.max_attempts, 3);
        assert!(
            report
                .summary(HealthThresholds::default())
                .contains("embedding unavailable")
        );

        sqlx::query("update projection_jobs set attempts = 0, updated_at = now() where last_error is not null")
            .execute(&mut *tx).await?;
        let changed: serde_json::Value = sqlx::query_scalar(QUERY).fetch_one(&mut *tx).await?;
        let report = QueueStatus::parse(&serde_json::to_vec(&changed)?)?;
        assert_eq!(report.oldest_seconds, 300);
        assert!(
            !report.healthy(HealthThresholds::default()),
            "fresh input must not hide old queued work"
        );

        sqlx::query("update projection_jobs set attempts = 2, queued_at = now() - interval '299 seconds' where last_error is not null")
            .execute(&mut *tx).await?;
        let retry: serde_json::Value = sqlx::query_scalar(QUERY).fetch_one(&mut *tx).await?;
        assert!(
            QueueStatus::parse(&serde_json::to_vec(&retry)?)?.healthy(HealthThresholds::default())
        );
        sqlx::query("delete from projection_jobs")
            .execute(&mut *tx)
            .await?;
        let drained: serde_json::Value = sqlx::query_scalar(QUERY).fetch_one(&mut *tx).await?;
        assert!(
            QueueStatus::parse(&serde_json::to_vec(&drained)?)?
                .healthy(HealthThresholds::default())
        );
        tx.rollback().await?;
        Ok(())
    }
}
