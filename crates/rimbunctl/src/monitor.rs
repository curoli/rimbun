use std::{
    collections::VecDeque,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::{Path, PathBuf},
    time::Duration,
};

use chrono::{DateTime, Utc};
use nix::fcntl::{Flock, FlockArg};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const MAX_STATE_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub webhook_url_env: Option<EnvName>,
}

impl Config {
    pub fn merge(&mut self, overlay: &Self) {
        if overlay.webhook_url_env.is_some() {
            self.webhook_url_env = overlay.webhook_url_env.clone();
        }
    }

    pub fn webhook_value(
        &self,
        profile_env: &std::collections::BTreeMap<String, String>,
        inherited: impl FnOnce(&str) -> Option<String>,
    ) -> Result<String, MonitorError> {
        let name = self
            .webhook_url_env
            .as_ref()
            .ok_or(MonitorError::NotConfigured)?;
        profile_env
            .get(&name.0)
            .cloned()
            .or_else(|| inherited(&name.0))
            .ok_or(MonitorError::InvalidWebhook)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(try_from = "String")]
pub struct EnvName(pub String);

impl TryFrom<String> for EnvName {
    type Error = &'static str;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let valid = value.bytes().enumerate().all(|(index, byte)| {
            byte == b'_' || byte.is_ascii_alphabetic() || (index > 0 && byte.is_ascii_digit())
        });
        if value.is_empty() || !valid {
            return Err("webhook_url_env must name an environment variable, not a URL");
        }
        Ok(Self(value))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Health {
    Healthy,
    Degraded,
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Problem {
    pub component: String,
    pub service: Option<String>,
    pub code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observation {
    pub profile: String,
    pub observed_at: DateTime<Utc>,
    pub overall: Health,
    pub problems: Vec<Problem>,
}

impl Observation {
    fn normalize(&mut self) {
        self.problems.sort();
        self.problems.dedup();
    }

    fn same_condition(&self, other: &Self) -> bool {
        self.overall == other.overall && self.problems == other.problems
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    Problem,
    Recovery,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub schema_version: u32,
    pub event_id: uuid::Uuid,
    pub kind: EventKind,
    pub previous: Option<Observation>,
    pub current: Observation,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct State {
    schema_version: u32,
    profile: String,
    last_observed: Option<Observation>,
    pending: VecDeque<Event>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum MonitorError {
    NotConfigured,
    Locked,
    StateIo,
    InvalidState,
    WrongProfile,
    StateTooLarge,
    InvalidWebhook,
}

impl std::fmt::Display for MonitorError {
    fn fmt(&self, output: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        output.write_str(match self {
            Self::NotConfigured => "monitor.webhook_url_env is not configured",
            Self::Locked => "another monitor is already running for this profile",
            Self::StateIo => "monitor state could not be read or saved",
            Self::InvalidState => {
                "monitor state is invalid or has an unsupported version; not reset"
            }
            Self::WrongProfile => "monitor state belongs to a different profile",
            Self::StateTooLarge => {
                "monitor state exceeds the 4 MiB limit; pending events were not discarded"
            }
            Self::InvalidWebhook => "monitor webhook URL is missing or invalid",
        })
    }
}

impl std::error::Error for MonitorError {}

pub struct Monitor {
    directory: PathBuf,
    state: State,
    _lock: Flock<File>,
}

pub struct Outcome {
    pub delivery: &'static str,
    pub pending_events: usize,
    pub event_id: Option<uuid::Uuid>,
    pub delivery_error: Option<DeliveryError>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum DeliveryError {
    Timeout,
    RequestFailed,
    HttpStatus(u16),
}

impl std::fmt::Display for DeliveryError {
    fn fmt(&self, output: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Timeout => output.write_str("webhook request timed out"),
            Self::RequestFailed => output.write_str("webhook request failed"),
            Self::HttpStatus(status) => write!(output, "webhook returned HTTP {status}"),
        }
    }
}

impl Monitor {
    pub fn state_path(&self) -> PathBuf {
        self.directory.join("state.json")
    }
    pub fn open(state_dir: &Path, profile: &str) -> Result<Self, MonitorError> {
        Self::open_with_directory_sync(state_dir, profile, |path| File::open(path)?.sync_all())
    }

    fn open_with_directory_sync(
        state_dir: &Path,
        profile: &str,
        mut sync_directory: impl FnMut(&Path) -> std::io::Result<()>,
    ) -> Result<Self, MonitorError> {
        let key = format!("{:x}", Sha256::digest(profile.as_bytes()));
        let directory = std::path::absolute(state_dir.join("monitor").join(key))
            .map_err(|_| MonitorError::StateIo)?;
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&directory)
            .map_err(|_| MonitorError::StateIo)?;
        // Each directory entry needs its parent's fsync, including state directories
        // created by the caller or left behind by a previous failed initialization.
        for ancestor in directory.ancestors() {
            sync_directory(ancestor).map_err(|_| MonitorError::StateIo)?;
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK)
            .open(directory.join("lock"))
            .map_err(|_| MonitorError::StateIo)?;
        if !file
            .metadata()
            .map_err(|_| MonitorError::StateIo)?
            .is_file()
        {
            return Err(MonitorError::StateIo);
        }
        let lock = Flock::lock(file, FlockArg::LockExclusiveNonblock).map_err(|(_, error)| {
            if error == nix::errno::Errno::EWOULDBLOCK {
                MonitorError::Locked
            } else {
                MonitorError::StateIo
            }
        })?;
        let state = match OpenOptions::new()
            .read(true)
            .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK)
            .open(directory.join("state.json"))
        {
            Ok(file) => {
                if !file
                    .metadata()
                    .map_err(|_| MonitorError::StateIo)?
                    .is_file()
                {
                    return Err(MonitorError::InvalidState);
                }
                let mut encoded = Vec::new();
                file.take(MAX_STATE_BYTES + 1)
                    .read_to_end(&mut encoded)
                    .map_err(|_| MonitorError::StateIo)?;
                if encoded.len() as u64 > MAX_STATE_BYTES {
                    return Err(MonitorError::StateTooLarge);
                }
                let state: State =
                    serde_json::from_slice(&encoded).map_err(|_| MonitorError::InvalidState)?;
                if state.schema_version != 1 {
                    return Err(MonitorError::InvalidState);
                }
                if state.profile != profile
                    || state
                        .last_observed
                        .as_ref()
                        .is_some_and(|item| item.profile != profile)
                    || state.pending.iter().any(|event| {
                        event.schema_version != 1
                            || event.current.profile != profile
                            || event
                                .previous
                                .as_ref()
                                .is_some_and(|item| item.profile != profile)
                    })
                {
                    return Err(MonitorError::WrongProfile);
                }
                state
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => State {
                schema_version: 1,
                profile: profile.to_owned(),
                ..State::default()
            },
            Err(_) => return Err(MonitorError::StateIo),
        };
        Ok(Self {
            directory,
            state,
            _lock: lock,
        })
    }

    fn save(&self) -> Result<(), MonitorError> {
        let encoded =
            serde_json::to_vec_pretty(&self.state).map_err(|_| MonitorError::InvalidState)?;
        if encoded.len() as u64 > MAX_STATE_BYTES {
            return Err(MonitorError::StateTooLarge);
        }
        let temporary = self
            .directory
            .join(format!("state-{}.tmp", uuid::Uuid::new_v4()));
        let result = (|| {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&temporary)?;
            file.write_all(&encoded)?;
            file.sync_all()?;
            fs::rename(&temporary, self.directory.join("state.json"))?;
            File::open(&self.directory)?.sync_all()
        })();
        let _ = fs::remove_file(&temporary);
        result.map_err(|_: std::io::Error| MonitorError::StateIo)
    }

    pub fn run_once(
        &mut self,
        mut current: Observation,
        mut send: impl FnMut(&Event) -> Result<(), DeliveryError>,
    ) -> Result<Outcome, MonitorError> {
        if current.profile != self.state.profile {
            return Err(MonitorError::WrongProfile);
        }
        current.normalize();
        let changed = self
            .state
            .last_observed
            .as_ref()
            .is_none_or(|previous| !previous.same_condition(&current));
        if changed && (current.overall != Health::Healthy || self.state.last_observed.is_some()) {
            self.state.pending.push_back(Event {
                schema_version: 1,
                event_id: uuid::Uuid::new_v4(),
                kind: if current.overall == Health::Healthy {
                    EventKind::Recovery
                } else {
                    EventKind::Problem
                },
                previous: self.state.last_observed.clone(),
                current: current.clone(),
            });
        }
        self.state.last_observed = Some(current);
        // Save the observation and outbox before making any externally visible request.
        self.save()?;
        let Some(event) = self.state.pending.front() else {
            return Ok(Outcome {
                delivery: "no_pending_events",
                pending_events: 0,
                event_id: None,
                delivery_error: None,
            });
        };
        let event_id = Some(event.event_id);
        if let Err(error) = send(event) {
            return Ok(Outcome {
                delivery: "failed",
                pending_events: self.state.pending.len(),
                event_id,
                delivery_error: Some(error),
            });
        }
        self.state.pending.pop_front();
        self.save()?;
        Ok(Outcome {
            delivery: "sent",
            pending_events: self.state.pending.len(),
            event_id,
            delivery_error: None,
        })
    }
}

pub struct Webhook {
    url: reqwest::Url,
}

impl Webhook {
    pub fn new(value: &str) -> Result<Self, MonitorError> {
        let url = reqwest::Url::parse(value).map_err(|_| MonitorError::InvalidWebhook)?;
        if !matches!(url.scheme(), "http" | "https")
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
        {
            return Err(MonitorError::InvalidWebhook);
        }
        Ok(Self { url })
    }

    pub fn send(&self, event: &Event) -> Result<(), DeliveryError> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| DeliveryError::RequestFailed)?;
        let result = runtime.block_on(async {
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(5))
                .connect_timeout(Duration::from_secs(2))
                .redirect(reqwest::redirect::Policy::none())
                .no_proxy()
                .build()
                .map_err(|_| DeliveryError::RequestFailed)?;
            let response = client
                .post(self.url.clone())
                .header("Idempotency-Key", event.event_id.to_string())
                .json(event)
                .send()
                .await
                .map_err(|error| {
                    if error.is_timeout() {
                        DeliveryError::Timeout
                    } else {
                        DeliveryError::RequestFailed
                    }
                })?;
            if response.status().is_success() {
                Ok(())
            } else {
                Err(DeliveryError::HttpStatus(response.status().as_u16()))
            }
        });
        // A timed-out blocking DNS resolver must not delay runtime shutdown.
        runtime.shutdown_background();
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{BufRead, BufReader},
        net::TcpListener,
        os::unix::fs::PermissionsExt,
    };

    struct Directory(PathBuf);
    impl Directory {
        fn new() -> Self {
            let path =
                std::env::temp_dir().join(format!("rimbun-monitor-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn observation(health: Health, code: &str) -> Observation {
        Observation {
            profile: "test".to_owned(),
            observed_at: Utc::now(),
            overall: health,
            problems: if code.is_empty() {
                vec![]
            } else {
                vec![Problem {
                    component: "projection_jobs".to_owned(),
                    service: None,
                    code: code.to_owned(),
                }]
            },
        }
    }

    #[test]
    fn first_open_syncs_the_entire_new_directory_chain_before_delivery() {
        let directory = Directory::new();
        let state_dir = directory.0.join("missing/state/profile");
        let state_file = state_dir
            .join("monitor")
            .join(format!("{:x}", Sha256::digest(b"test")))
            .join("state.json");
        assert!(!state_dir.exists());
        let mut synced = Vec::new();
        let mut monitor = Monitor::open_with_directory_sync(&state_dir, "test", |path| {
            assert!(path.is_dir());
            assert!(!state_file.exists());
            File::open(path)?.sync_all()?;
            synced.push(path.to_path_buf());
            Ok(())
        })
        .unwrap();
        let expected: Vec<_> = monitor
            .directory
            .ancestors()
            .map(Path::to_path_buf)
            .collect();
        assert_eq!(synced, expected);
        assert_eq!(synced.last().unwrap(), Path::new("/"));
        monitor
            .run_once(observation(Health::Degraded, "first"), |_| {
                assert_eq!(synced, expected);
                Err(DeliveryError::RequestFailed)
            })
            .unwrap();
        drop(monitor);
        let recovered = Monitor::open(&state_dir, "test").unwrap();
        assert_eq!(recovered.state.pending.len(), 1);
    }

    #[test]
    fn each_directory_sync_failure_prevents_delivery_and_is_retried() {
        let fixture = Directory::new();
        let state_dir = fixture.0.join("missing/state/profile");
        let leaf = state_dir
            .join("monitor")
            .join(format!("{:x}", Sha256::digest(b"test")));
        let expected: Vec<_> = leaf.ancestors().map(Path::to_path_buf).collect();
        for failed_at in 0..expected.len() {
            fs::remove_dir_all(fixture.0.join("missing"))
                .or_else(|error| {
                    if error.kind() == std::io::ErrorKind::NotFound {
                        Ok(())
                    } else {
                        Err(error)
                    }
                })
                .unwrap();
            let mut synced = Vec::new();
            let result = Monitor::open_with_directory_sync(&state_dir, "test", |path| {
                synced.push(path.to_path_buf());
                if synced.len() == failed_at + 1 {
                    return Err(std::io::Error::from_raw_os_error(nix::libc::EIO));
                }
                File::open(path)?.sync_all()
            })
            .and_then(|mut monitor| {
                monitor.run_once(observation(Health::Degraded, "first"), |_| {
                    panic!("directory sync failure must prevent HTTP delivery")
                })
            });
            assert!(matches!(result, Err(MonitorError::StateIo)));
            assert_eq!(synced, expected[..=failed_at]);
            assert!(!leaf.join("state.json").exists());
            synced.clear();
            let retried = Monitor::open_with_directory_sync(&state_dir, "test", |path| {
                File::open(path)?.sync_all()?;
                synced.push(path.to_path_buf());
                Ok(())
            })
            .unwrap();
            assert_eq!(
                synced, expected,
                "existing directories must be synced again"
            );
            drop(retried);
        }
    }

    #[test]
    fn webhook_value_uses_profile_precedence_without_global_environment_changes() {
        let config: Config = toml::from_str("webhook_url_env = 'ALERT_URL'").unwrap();
        let profile = std::collections::BTreeMap::from([(
            "ALERT_URL".to_owned(),
            "profile-value".to_owned(),
        )]);
        assert_eq!(
            config
                .webhook_value(&profile, |_| panic!("must not fall back"))
                .unwrap(),
            "profile-value"
        );
        assert_eq!(
            config
                .webhook_value(&std::collections::BTreeMap::new(), |_| Some(
                    "inherited-value".to_owned()
                ))
                .unwrap(),
            "inherited-value"
        );
        assert!(matches!(
            config.webhook_value(&std::collections::BTreeMap::new(), |_| None),
            Err(MonitorError::InvalidWebhook)
        ));
        assert!(matches!(
            Config::default().webhook_value(&profile, |_| None),
            Err(MonitorError::NotConfigured)
        ));
        let empty = std::collections::BTreeMap::from([("ALERT_URL".to_owned(), String::new())]);
        assert!(matches!(
            Webhook::new(
                &config
                    .webhook_value(&empty, |_| panic!(
                        "empty profile value overrides inheritance"
                    ))
                    .unwrap()
            ),
            Err(MonitorError::InvalidWebhook)
        ));
    }

    #[test]
    fn oversized_state_does_not_send_or_discard_already_queued_events() {
        let directory = Directory::new();
        let mut monitor = Monitor::open(&directory.0, "test").unwrap();
        let original = monitor
            .run_once(observation(Health::Degraded, "first"), |_| {
                Err(DeliveryError::RequestFailed)
            })
            .unwrap()
            .event_id
            .unwrap();
        let too_large = observation(Health::Degraded, &"x".repeat(MAX_STATE_BYTES as usize));
        assert!(matches!(
            monitor.run_once(too_large, |_| panic!("state must be saved first")),
            Err(MonitorError::StateTooLarge)
        ));
        drop(monitor);
        let mut recovered = Monitor::open(&directory.0, "test").unwrap();
        recovered
            .run_once(observation(Health::Degraded, "first"), |event| {
                assert_eq!(event.event_id, original);
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn baseline_changed_problems_and_recovery_do_not_repeat() {
        let directory = Directory::new();
        let mut monitor = Monitor::open(&directory.0, "test").unwrap();
        monitor
            .run_once(observation(Health::Healthy, ""), |_| {
                panic!("healthy baseline must not notify")
            })
            .unwrap();
        let mut events = Vec::new();
        for (health, code) in [
            (Health::Degraded, "projection_age_exceeded"),
            (Health::Degraded, "projection_age_exceeded"),
            (Health::Degraded, "projection_retry_limit_exceeded"),
            (Health::Healthy, ""),
            (Health::Healthy, ""),
        ] {
            let result = monitor
                .run_once(observation(health, code), |event| {
                    events.push(event.clone());
                    Ok(())
                })
                .unwrap();
            assert_eq!(result.pending_events, 0);
        }
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].kind, EventKind::Problem);
        assert_eq!(events[1].kind, EventKind::Problem);
        assert_eq!(events[2].kind, EventKind::Recovery);
        assert_eq!(
            events[2].previous.as_ref().unwrap().overall,
            Health::Degraded
        );
        assert_eq!(
            monitor.state.last_observed.as_ref().unwrap().overall,
            Health::Healthy
        );
    }

    #[test]
    fn failed_event_is_durable_and_retried_before_a_later_recovery() {
        let directory = Directory::new();
        let original_id;
        {
            let mut monitor = Monitor::open(&directory.0, "test").unwrap();
            let state_file = monitor.directory.join("state.json");
            let result = monitor
                .run_once(
                    observation(Health::Degraded, "projection_age_exceeded"),
                    |event| {
                        let disk: State =
                            serde_json::from_slice(&fs::read(&state_file).unwrap()).unwrap();
                        assert_eq!(
                            disk.pending[0].event_id, event.event_id,
                            "event must be on disk before HTTP"
                        );
                        Err(DeliveryError::HttpStatus(503))
                    },
                )
                .unwrap();
            assert_eq!(result.delivery, "failed");
            assert_eq!(result.pending_events, 1);
            original_id = result.event_id.unwrap();
            assert_eq!(
                fs::metadata(&state_file).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        let mut monitor = Monitor::open(&directory.0, "test").unwrap();
        let result = monitor
            .run_once(observation(Health::Healthy, ""), |event| {
                assert_eq!(event.event_id, original_id);
                assert_eq!(event.kind, EventKind::Problem);
                Ok(())
            })
            .unwrap();
        assert_eq!(result.pending_events, 1);
        assert_eq!(
            monitor.state.last_observed.as_ref().unwrap().overall,
            Health::Healthy
        );
        drop(monitor);
        let mut monitor = Monitor::open(&directory.0, "test").unwrap();
        monitor
            .run_once(observation(Health::Healthy, ""), |event| {
                assert_ne!(event.event_id, original_id);
                assert_eq!(event.kind, EventKind::Recovery);
                Ok(())
            })
            .unwrap();
        monitor
            .run_once(observation(Health::Healthy, ""), |_| {
                panic!("delivered recovery must not repeat")
            })
            .unwrap();
    }

    #[test]
    fn failed_acknowledgement_save_retries_the_same_durable_event() {
        let directory = Directory::new();
        let mut monitor = Monitor::open(&directory.0, "test").unwrap();
        let state_directory = monitor.directory.clone();
        let moved_directory = directory.0.join("saved-state");
        let mut delivered_id = None;
        assert!(matches!(
            monitor.run_once(observation(Health::Degraded, "first"), |event| {
                delivered_id = Some(event.event_id);
                // Simulate losing filesystem access after the receiver acknowledges.
                fs::rename(&state_directory, &moved_directory).unwrap();
                Ok(())
            }),
            Err(MonitorError::StateIo)
        ));
        drop(monitor);
        fs::rename(&moved_directory, &state_directory).unwrap();
        let mut recovered = Monitor::open(&directory.0, "test").unwrap();
        let outcome = recovered
            .run_once(observation(Health::Degraded, "first"), |event| {
                assert_eq!(Some(event.event_id), delivered_id);
                Ok(())
            })
            .unwrap();
        assert_eq!(outcome.pending_events, 0);
    }

    #[test]
    fn problem_order_duplicates_and_timestamps_do_not_create_new_events() {
        let directory = Directory::new();
        let mut monitor = Monitor::open(&directory.0, "test").unwrap();
        let mut first = observation(Health::Stopped, "service_not_ready");
        let mut other = first.problems[0].clone();
        other.code = "service_process_not_running".to_owned();
        first.problems.push(other);
        monitor.run_once(first.clone(), |_| Ok(())).unwrap();
        first.observed_at = Utc::now();
        first.problems.reverse();
        first.problems.push(first.problems[0].clone());
        monitor
            .run_once(first, |_| {
                panic!("same canonical condition must not notify")
            })
            .unwrap();
    }

    #[test]
    fn locks_are_exclusive_released_and_profile_scoped() {
        let directory = Directory::new();
        let first = Monitor::open(&directory.0, "test").unwrap();
        assert!(matches!(
            Monitor::open(&directory.0, "test"),
            Err(MonitorError::Locked)
        ));
        let other = Monitor::open(&directory.0, "other").unwrap();
        assert_ne!(first.directory, other.directory);
        drop(first);
        assert!(Monitor::open(&directory.0, "test").is_ok());
    }

    #[test]
    fn corrupt_or_wrong_profile_state_is_not_silently_reset() {
        let directory = Directory::new();
        let mut monitor = Monitor::open(&directory.0, "test").unwrap();
        monitor
            .run_once(observation(Health::Healthy, ""), |_| Ok(()))
            .unwrap();
        let state_file = monitor.directory.join("state.json");
        drop(monitor);
        for data in [
            "invalid",
            r#"{"schema_version":2,"profile":"test","last_observed":null,"pending":[]}"#,
        ] {
            fs::write(&state_file, data).unwrap();
            assert!(matches!(
                Monitor::open(&directory.0, "test"),
                Err(MonitorError::InvalidState)
            ));
            assert_eq!(fs::read_to_string(&state_file).unwrap(), data);
        }
        fs::write(
            &state_file,
            r#"{"schema_version":1,"profile":"other","last_observed":null,"pending":[]}"#,
        )
        .unwrap();
        assert!(matches!(
            Monitor::open(&directory.0, "test"),
            Err(MonitorError::WrongProfile)
        ));
    }

    #[test]
    fn failed_persistence_prevents_delivery_and_fifo_state_does_not_block() {
        let directory = Directory::new();
        let mut monitor = Monitor::open(&directory.0, "test").unwrap();
        let state_file = monitor.directory.join("state.json");
        fs::create_dir(&state_file).unwrap();
        assert!(matches!(
            monitor.run_once(
                observation(Health::Degraded, "projection_age_exceeded"),
                |_| panic!("unsaved events must not be sent")
            ),
            Err(MonitorError::StateIo)
        ));
        drop(monitor);
        fs::remove_dir(&state_file).unwrap();
        nix::unistd::mkfifo(
            &state_file,
            nix::sys::stat::Mode::S_IRUSR | nix::sys::stat::Mode::S_IWUSR,
        )
        .unwrap();
        assert!(matches!(
            Monitor::open(&directory.0, "test"),
            Err(MonitorError::InvalidState)
        ));
    }

    fn server(response: String) -> (String, std::thread::JoinHandle<(String, serde_json::Value)>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let thread = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut reader = BufReader::new(&mut stream);
            let mut headers = String::new();
            let mut length = 0;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" {
                    break;
                }
                if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = value.trim().parse().unwrap();
                }
                headers.push_str(&line);
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            stream.write_all(response.as_bytes()).unwrap();
            (headers, serde_json::from_slice(&body).unwrap())
        });
        (url, thread)
    }

    #[test]
    fn webhook_posts_versioned_events_and_idempotency_key() {
        let directory = Directory::new();
        let mut monitor = Monitor::open(&directory.0, "test").unwrap();
        let (url, server) = server(
            "HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_owned(),
        );
        let webhook = Webhook::new(&format!("{url}/hook?token=must-not-be-stored")).unwrap();
        let result = monitor
            .run_once(
                observation(Health::Degraded, "projection_age_exceeded"),
                |event| webhook.send(event),
            )
            .unwrap();
        let (headers, payload) = server.join().unwrap();
        assert!(
            headers
                .to_ascii_lowercase()
                .contains(&format!("idempotency-key: {}", result.event_id.unwrap()))
        );
        assert_eq!(payload["schema_version"], 1);
        assert_eq!(payload["kind"], "problem");
        assert_eq!(payload["current"]["overall"], "degraded");
        assert!(!payload.to_string().contains("must-not-be-stored"));
        assert!(
            !fs::read_to_string(monitor.directory.join("state.json"))
                .unwrap()
                .contains("must-not-be-stored")
        );
    }

    #[test]
    fn webhook_redirects_are_not_followed_and_errors_are_sanitized() {
        let target = TcpListener::bind("127.0.0.1:0").unwrap();
        target.set_nonblocking(true).unwrap();
        let (url, server) = server(format!(
            "HTTP/1.1 302 Found\r\nLocation: http://{}/secret\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            target.local_addr().unwrap()
        ));
        let webhook = Webhook::new(&url).unwrap();
        let event = Event {
            schema_version: 1,
            event_id: uuid::Uuid::new_v4(),
            kind: EventKind::Problem,
            previous: None,
            current: observation(Health::Degraded, "projection_age_exceeded"),
        };
        let error = webhook.send(&event).unwrap_err();
        server.join().unwrap();
        assert_eq!(error, DeliveryError::HttpStatus(302));
        assert_eq!(
            target.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
        assert!(!error.to_string().contains("secret"));
        for value in [
            "not a URL",
            "file:///tmp/token",
            "https://user:secret@example.test/hook",
            "https://example.test/#secret",
        ] {
            assert!(matches!(
                Webhook::new(value),
                Err(MonitorError::InvalidWebhook)
            ));
        }
    }

    #[test]
    fn webhook_response_timeout_is_bounded_and_does_not_expose_the_url() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/secret-token", listener.local_addr().unwrap());
        let (release, waiting) = std::sync::mpsc::channel();
        let server = std::thread::spawn(move || {
            let (_stream, _) = listener.accept().unwrap();
            let _ = waiting.recv_timeout(Duration::from_secs(8));
        });
        let webhook = Webhook::new(&url).unwrap();
        let event = Event {
            schema_version: 1,
            event_id: uuid::Uuid::new_v4(),
            kind: EventKind::Problem,
            previous: None,
            current: observation(Health::Degraded, "projection_age_exceeded"),
        };
        let started = std::time::Instant::now();
        let result = webhook.send(&event);
        let _ = release.send(());
        server.join().unwrap();
        assert_eq!(result.unwrap_err(), DeliveryError::Timeout);
        assert!(started.elapsed() < Duration::from_secs(8));
        assert!(!DeliveryError::Timeout.to_string().contains("secret-token"));
    }
}
