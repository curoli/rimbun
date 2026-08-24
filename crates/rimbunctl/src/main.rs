use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    io::{BufRead, BufReader, ErrorKind, Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    os::unix::{fs::symlink, process::CommandExt},
    path::{Path, PathBuf},
    process::ExitCode,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, anyhow, bail};
use chrono::Utc;
use clap::{Parser, Subcommand, ValueEnum};
use nix::{
    sys::signal::{Signal, kill, killpg},
    unistd::Pid,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const SERVICE_ORDER: [ServiceName; 4] = [
    ServiceName::Db,
    ServiceName::Embedding,
    ServiceName::Backend,
    ServiceName::Frontend,
];
const READINESS_TIMEOUT: Duration = Duration::from_secs(300);
const READINESS_POLL_INTERVAL: Duration = Duration::from_millis(250);
const READINESS_REPORT_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Debug, Parser)]
#[command(name = "rimbunctl")]
struct Cli {
    profile: Option<String>,
    #[command(subcommand)]
    command: CommandKind,
}

#[derive(Debug, Subcommand)]
enum CommandKind {
    ListProfiles,
    Status,
    Check,
    Deploy {
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        allow_dirty: bool,
    },
    Releases,
    Rollback {
        release: String,
        #[arg(long)]
        dry_run: bool,
    },
    ListUsers,
    ExportContributions {
        username: String,
        file: Option<String>,
    },
    ImportContributions {
        username: String,
        file: String,
        #[arg(long)]
        publish: bool,
    },
    Start {
        #[arg(default_value = "all")]
        service: ServiceTarget,
        #[arg(long)]
        source: bool,
    },
    Stop {
        #[arg(default_value = "all")]
        service: ServiceTarget,
    },
    Restart {
        #[arg(default_value = "all")]
        service: ServiceTarget,
        #[arg(long)]
        source: bool,
    },
    Log {
        #[arg(default_value = "all")]
        service: ServiceTarget,
        #[arg(long, short)]
        follow: bool,
    },
    Backup {
        name: Option<String>,
    },
    VerifyBackup {
        backup: String,
    },
    Restore {
        backup: String,
        #[arg(long)]
        allow_profile_mismatch: bool,
    },
    SetRole {
        username: String,
        role: String,
    },
    SetPassword {
        username: String,
        new_password: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
enum ServiceName {
    Db,
    Embedding,
    Backend,
    Frontend,
}

impl ServiceName {
    fn as_str(self) -> &'static str {
        match self {
            ServiceName::Db => "db",
            ServiceName::Embedding => "embedding",
            ServiceName::Backend => "backend",
            ServiceName::Frontend => "frontend",
        }
    }
}

impl std::str::FromStr for ServiceName {
    type Err = String;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        match value {
            "db" => Ok(Self::Db),
            "embedding" => Ok(Self::Embedding),
            "backend" => Ok(Self::Backend),
            "frontend" => Ok(Self::Frontend),
            _ => Err(format!("unknown service '{value}'")),
        }
    }
}

#[derive(Debug, Clone)]
enum ServiceTarget {
    All,
    One(ServiceName),
}

impl std::str::FromStr for ServiceTarget {
    type Err = String;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        if value == "all" {
            return Ok(Self::All);
        }
        Ok(Self::One(value.parse()?))
    }
}

#[derive(Debug, Deserialize, Default)]
struct FileConfig {
    #[serde(default)]
    fragments: BTreeMap<String, LayerConfig>,
    #[serde(default)]
    profiles: BTreeMap<String, ProfileConfig>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct LayerConfig {
    #[serde(default)]
    extends: Vec<String>,
    state_namespace: Option<String>,
    #[serde(default)]
    vars: BTreeMap<String, String>,
    #[serde(default)]
    env: BTreeMap<String, String>,
    #[serde(default)]
    services: BTreeMap<String, ServiceConfig>,
    database: Option<DatabaseConfig>,
    deployment: Option<DeploymentConfig>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct ProfileConfig {
    #[serde(default)]
    extends: Vec<String>,
    state_namespace: Option<String>,
    #[serde(default)]
    vars: BTreeMap<String, String>,
    #[serde(default)]
    env: BTreeMap<String, String>,
    #[serde(default)]
    services: BTreeMap<String, ServiceConfig>,
    database: Option<DatabaseConfig>,
    deployment: Option<DeploymentConfig>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct ServiceConfig {
    workdir: Option<String>,
    run: Option<String>,
    bootstrap: Option<String>,
    stop: Option<String>,
    depends_on: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct DatabaseConfig {
    backup: Option<String>,
    restore: Option<String>,
    verify: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct DeploymentConfig {
    build: Option<Vec<String>>,
    migrate: Option<String>,
    #[serde(default)]
    artifacts: BTreeMap<String, String>,
    #[serde(default)]
    run: BTreeMap<String, String>,
    retention: Option<usize>,
}

#[derive(Debug, Clone, Default)]
struct ResolvedProfile {
    profile_name: String,
    state_namespace: String,
    vars: BTreeMap<String, String>,
    env: BTreeMap<String, String>,
    services: BTreeMap<ServiceName, ResolvedServiceConfig>,
    database: Option<ResolvedDatabaseConfig>,
    deployment: Option<ResolvedDeploymentConfig>,
}

#[derive(Debug, Clone, Default)]
struct ResolvedServiceConfig {
    workdir: String,
    run: String,
    bootstrap: Option<String>,
    stop: Option<String>,
    depends_on: Vec<ServiceName>,
}

#[derive(Debug, Clone, Default)]
struct ResolvedDatabaseConfig {
    backup: String,
    restore: String,
    verify: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct ResolvedDeploymentConfig {
    build: Vec<String>,
    migrate: String,
    artifacts: BTreeMap<String, String>,
    run: BTreeMap<ServiceName, String>,
    retention: usize,
}

#[derive(Debug)]
struct ConfigRegistry {
    fragments: BTreeMap<String, LayerConfig>,
    profiles: BTreeMap<String, ProfileConfig>,
}

#[derive(Debug)]
struct Paths {
    repo_root: PathBuf,
    state_dir: PathBuf,
    backup_dir: PathBuf,
    log_dir: PathBuf,
    pid_dir: PathBuf,
    release_dir: PathBuf,
}

#[derive(Debug)]
struct ServicePids {
    service_pid: i32,
}

#[derive(Debug, Deserialize, Serialize)]
struct BackupMetadata {
    format_version: u32,
    profile: String,
    database: String,
    created_at: String,
    size_bytes: u64,
    sha256: String,
    verification: BackupVerification,
}

#[derive(Debug, Deserialize, Serialize)]
struct BackupVerification {
    status: String,
    verified_at: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OverallStatus {
    Healthy,
    Degraded,
    Stopped,
}

impl OverallStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Healthy => "healthy",
            Self::Degraded => "degraded",
            Self::Stopped => "stopped",
        }
    }
}

#[derive(Debug)]
struct ServiceStatusReport {
    service: ServiceName,
    process: &'static str,
    pid: Option<i32>,
    readiness: &'static str,
    endpoint: String,
    log_path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CheckOutcome {
    Pass,
    Fail,
    Skip,
}

#[derive(Debug)]
struct CheckResult {
    name: String,
    outcome: CheckOutcome,
    detail: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct ReleaseRecord {
    format_version: u32,
    id: String,
    profile: String,
    git_commit: String,
    #[serde(default)]
    worktree_dirty: bool,
    started_at: String,
    completed_at: Option<String>,
    status: String,
    backup: Option<String>,
    #[serde(default)]
    artifact_dir: Option<String>,
    #[serde(default)]
    activated_at: Option<String>,
    #[serde(default)]
    artifact_checksums: BTreeMap<String, String>,
    phases: Vec<ReleasePhase>,
}

struct DeploymentLock {
    path: PathBuf,
}

impl Drop for DeploymentLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct ReleasePhase {
    name: String,
    status: String,
    completed_at: String,
    detail: Option<String>,
}

fn repo_root() -> Result<PathBuf> {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .ok_or_else(|| anyhow!("failed to determine repository root"))
}

fn state_paths(state_namespace: &str) -> Result<Paths> {
    let repo_root = repo_root()?;
    let state_dir = repo_root.join(".rimbun").join(state_namespace);
    Ok(Paths {
        repo_root,
        backup_dir: state_dir.join("backups"),
        log_dir: state_dir.join("logs"),
        pid_dir: state_dir.join("pids"),
        release_dir: state_dir.join("releases"),
        state_dir,
    })
}

fn ensure_state_dirs(paths: &Paths) -> Result<()> {
    fs::create_dir_all(&paths.backup_dir)?;
    fs::create_dir_all(&paths.log_dir)?;
    fs::create_dir_all(&paths.pid_dir)?;
    fs::create_dir_all(&paths.release_dir)?;
    Ok(())
}

fn builtin_registry() -> ConfigRegistry {
    let mut fragments = BTreeMap::new();

    fragments.insert("dev".to_owned(), LayerConfig::default());

    fragments.insert(
        "local-docker".to_owned(),
        LayerConfig {
            services: BTreeMap::from([(
                "db".to_owned(),
                ServiceConfig {
                    workdir: Some(".".to_owned()),
                    bootstrap: Some("docker compose up -d postgres".to_owned()),
                    run: Some("docker compose logs -f postgres".to_owned()),
                    stop: Some("docker compose stop postgres >/dev/null".to_owned()),
                    depends_on: Some(vec![]),
                },
            )]),
            database: Some(DatabaseConfig {
                backup: Some(
                    "docker compose exec -T postgres pg_dump -U postgres -d {db_name} > {file}"
                        .to_owned(),
                ),
                restore: Some(
                    "docker compose exec -T postgres psql -U postgres -d {db_name} < {file}"
                        .to_owned(),
                ),
                verify: Some(
                    "set -e; cleanup() { docker compose exec -T postgres dropdb -U postgres --if-exists {verification_db} >/dev/null; }; trap cleanup EXIT; docker compose exec -T postgres createdb -U postgres {verification_db}; docker compose exec -T postgres psql -U postgres -d {verification_db} -v ON_ERROR_STOP=1 < {file} >/dev/null; test \"$(docker compose exec -T postgres psql -U postgres -d {verification_db} -tAc \"SELECT count(*) FROM pg_class WHERE relkind = 'r' AND relname IN ('users', 'documents', '_sqlx_migrations')\")\" = \"3\""
                        .to_owned(),
                ),
            }),
            ..LayerConfig::default()
        },
    );

    fragments.insert(
        "rimbun-local".to_owned(),
        LayerConfig {
            vars: BTreeMap::from([
                ("db_name".to_owned(), "rimbun".to_owned()),
                ("backend_port".to_owned(), "3000".to_owned()),
                ("frontend_port".to_owned(), "5173".to_owned()),
                ("embedding_port".to_owned(), "8001".to_owned()),
            ]),
            env: BTreeMap::from([
                (
                    "DATABASE_URL".to_owned(),
                    "postgres://postgres:postgres@127.0.0.1:5432/{db_name}".to_owned(),
                ),
                ("RIMBUN_PORT".to_owned(), "{backend_port}".to_owned()),
                (
                    "RIMBUN_EMBEDDING_PORT".to_owned(),
                    "{embedding_port}".to_owned(),
                ),
                (
                    "EMBEDDING_SERVICE_URL".to_owned(),
                    "http://127.0.0.1:{embedding_port}".to_owned(),
                ),
            ]),
            services: BTreeMap::from([
                (
                    "embedding".to_owned(),
                    ServiceConfig {
                        workdir: Some(".".to_owned()),
                        run: Some(
                            "cargo run -p rimbun-embedding-service --bin rimbun-embedding-service"
                                .to_owned(),
                        ),
                        depends_on: Some(vec![]),
                        ..ServiceConfig::default()
                    },
                ),
                (
                    "backend".to_owned(),
                    ServiceConfig {
                        workdir: Some(".".to_owned()),
                        run: Some("cargo run -p rimbun-api --bin rimbun-api".to_owned()),
                        depends_on: Some(vec!["db".to_owned(), "embedding".to_owned()]),
                        ..ServiceConfig::default()
                    },
                ),
                (
                    "frontend".to_owned(),
                    ServiceConfig {
                        workdir: Some("web".to_owned()),
                        bootstrap: Some("test -d node_modules || npm install".to_owned()),
                        run: Some(
                            "npm run dev -- --host 127.0.0.1 --port {frontend_port} < /dev/null"
                                .to_owned(),
                        ),
                        depends_on: Some(vec![]),
                        ..ServiceConfig::default()
                    },
                ),
            ]),
            deployment: Some(DeploymentConfig {
                build: Some(vec![
                    "cargo build --workspace".to_owned(),
                    "npm run build --prefix web".to_owned(),
                ]),
                migrate: Some("{release_dir}/bin/rimbun-migrate".to_owned()),
                artifacts: BTreeMap::from([
                    ("backend".to_owned(), "target/debug/rimbun-api".to_owned()),
                    (
                        "embedding".to_owned(),
                        "target/debug/rimbun-embedding-service".to_owned(),
                    ),
                    (
                        "migrate".to_owned(),
                        "target/debug/rimbun-migrate".to_owned(),
                    ),
                    (
                        "static".to_owned(),
                        "target/debug/rimbun-static-server".to_owned(),
                    ),
                    ("frontend".to_owned(), "web/dist".to_owned()),
                ]),
                run: BTreeMap::from([
                    (
                        "embedding".to_owned(),
                        "{release_dir}/bin/rimbun-embedding-service".to_owned(),
                    ),
                    (
                        "backend".to_owned(),
                        "{release_dir}/bin/rimbun-api".to_owned(),
                    ),
                    (
                        "frontend".to_owned(),
                        "{release_dir}/bin/rimbun-static-server {release_dir}/web {frontend_port}"
                            .to_owned(),
                    ),
                ]),
                retention: Some(5),
            }),
            ..LayerConfig::default()
        },
    );

    let mut profiles = BTreeMap::new();
    profiles.insert(
        "dev".to_owned(),
        ProfileConfig {
            extends: vec![
                "dev".to_owned(),
                "local-docker".to_owned(),
                "rimbun-local".to_owned(),
            ],
            state_namespace: Some("dev".to_owned()),
            ..ProfileConfig::default()
        },
    );

    ConfigRegistry {
        fragments,
        profiles,
    }
}

fn load_registry(repo_root: &Path) -> Result<ConfigRegistry> {
    let mut registry = builtin_registry();
    let config_path = repo_root.join("rimbunctl.toml");
    if config_path.exists() {
        let raw = fs::read_to_string(&config_path)
            .with_context(|| format!("failed to read {}", config_path.display()))?;
        let file_config: FileConfig = toml::from_str(&raw)
            .with_context(|| format!("failed to parse {}", config_path.display()))?;
        registry.fragments.extend(file_config.fragments);
        registry.profiles.extend(file_config.profiles);
    }
    Ok(registry)
}

fn list_profiles(registry: &ConfigRegistry) {
    for profile in registry.profiles.keys() {
        println!("{profile}");
    }
}

fn merge_service(base: &mut ServiceConfig, overlay: &ServiceConfig) {
    if let Some(workdir) = &overlay.workdir {
        base.workdir = Some(workdir.clone());
    }
    if let Some(run) = &overlay.run {
        base.run = Some(run.clone());
    }
    if let Some(bootstrap) = &overlay.bootstrap {
        base.bootstrap = Some(bootstrap.clone());
    }
    if let Some(stop) = &overlay.stop {
        base.stop = Some(stop.clone());
    }
    if let Some(depends_on) = &overlay.depends_on {
        base.depends_on = Some(depends_on.clone());
    }
}

fn merge_database(base: &mut DatabaseConfig, overlay: &DatabaseConfig) {
    if let Some(backup) = &overlay.backup {
        base.backup = Some(backup.clone());
    }
    if let Some(restore) = &overlay.restore {
        base.restore = Some(restore.clone());
    }
    if let Some(verify) = &overlay.verify {
        base.verify = Some(verify.clone());
    }
}

fn merge_deployment(base: &mut DeploymentConfig, overlay: &DeploymentConfig) {
    if let Some(build) = &overlay.build {
        base.build = Some(build.clone());
    }
    if let Some(migrate) = &overlay.migrate {
        base.migrate = Some(migrate.clone());
    }
    base.artifacts.extend(overlay.artifacts.clone());
    base.run.extend(overlay.run.clone());
    if let Some(retention) = overlay.retention {
        base.retention = Some(retention);
    }
}

fn merge_layer(base: &mut LayerConfig, overlay: &LayerConfig) {
    if let Some(namespace) = &overlay.state_namespace {
        base.state_namespace = Some(namespace.clone());
    }
    base.vars.extend(overlay.vars.clone());
    base.env.extend(overlay.env.clone());
    for (name, service) in &overlay.services {
        merge_service(base.services.entry(name.clone()).or_default(), service);
    }
    if let Some(database) = &overlay.database {
        if let Some(base_database) = &mut base.database {
            merge_database(base_database, database);
        } else {
            base.database = Some(database.clone());
        }
    }
    if let Some(deployment) = &overlay.deployment {
        if let Some(base_deployment) = &mut base.deployment {
            merge_deployment(base_deployment, deployment);
        } else {
            base.deployment = Some(deployment.clone());
        }
    }
}

fn resolve_fragment(
    registry: &ConfigRegistry,
    name: &str,
    stack: &mut Vec<String>,
) -> Result<LayerConfig> {
    if stack.iter().any(|entry| entry == name) {
        stack.push(name.to_owned());
        bail!("cyclic profile inheritance: {}", stack.join(" -> "));
    }

    let layer = if let Some(fragment) = registry.fragments.get(name) {
        fragment.clone()
    } else if let Some(profile) = registry.profiles.get(name) {
        LayerConfig {
            extends: profile.extends.clone(),
            state_namespace: profile.state_namespace.clone(),
            vars: profile.vars.clone(),
            env: profile.env.clone(),
            services: profile.services.clone(),
            database: profile.database.clone(),
            deployment: profile.deployment.clone(),
        }
    } else {
        bail!("unknown fragment/profile '{name}'");
    };

    stack.push(name.to_owned());
    let mut resolved = LayerConfig::default();
    for parent in &layer.extends {
        let parent_layer = resolve_fragment(registry, parent, stack)?;
        merge_layer(&mut resolved, &parent_layer);
    }
    let _ = stack.pop();

    let mut self_layer = layer;
    self_layer.extends.clear();
    merge_layer(&mut resolved, &self_layer);
    Ok(resolved)
}

fn interpolate_template(template: &str, vars: &BTreeMap<String, String>) -> String {
    let mut current = template.to_owned();
    for _ in 0..8 {
        let mut next = current.clone();
        for (key, value) in vars {
            next = next.replace(&format!("{{{key}}}"), value);
        }
        if next == current {
            return current;
        }
        current = next;
    }
    current
}

fn interpolate_map(
    map: &BTreeMap<String, String>,
    vars: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    map.iter()
        .map(|(key, value)| (key.clone(), interpolate_template(value, vars)))
        .collect()
}

fn resolve_profile(registry: &ConfigRegistry, profile_name: &str) -> Result<ResolvedProfile> {
    let profile = registry
        .profiles
        .get(profile_name)
        .ok_or_else(|| anyhow!("unsupported profile '{profile_name}'"))?
        .clone();

    let mut resolved_layer = LayerConfig::default();
    let mut stack = Vec::new();
    for parent in &profile.extends {
        let parent_layer = resolve_fragment(registry, parent, &mut stack)?;
        merge_layer(&mut resolved_layer, &parent_layer);
    }

    merge_layer(
        &mut resolved_layer,
        &LayerConfig {
            extends: vec![],
            state_namespace: profile.state_namespace.clone(),
            vars: profile.vars.clone(),
            env: profile.env.clone(),
            services: profile.services.clone(),
            database: profile.database.clone(),
            deployment: profile.deployment.clone(),
        },
    );

    let state_namespace = resolved_layer
        .state_namespace
        .clone()
        .unwrap_or_else(|| profile_name.to_owned());
    let paths = state_paths(&state_namespace)?;

    let mut vars = resolved_layer.vars.clone();
    vars.insert("profile".to_owned(), profile_name.to_owned());
    vars.insert("state_namespace".to_owned(), state_namespace.clone());
    vars.insert(
        "repo_root".to_owned(),
        paths.repo_root.display().to_string(),
    );
    vars.insert(
        "state_dir".to_owned(),
        paths.state_dir.display().to_string(),
    );
    vars.insert(
        "backup_dir".to_owned(),
        paths.backup_dir.display().to_string(),
    );
    vars.insert("log_dir".to_owned(), paths.log_dir.display().to_string());
    vars.insert("pid_dir".to_owned(), paths.pid_dir.display().to_string());

    let vars = interpolate_map(&vars, &vars);
    let env = interpolate_map(&resolved_layer.env, &vars);

    let mut services = BTreeMap::new();
    for (name, service) in resolved_layer.services {
        let service_name: ServiceName = name.parse().map_err(|message: String| anyhow!(message))?;
        let workdir = service
            .workdir
            .ok_or_else(|| anyhow!("service '{}' missing workdir", service_name.as_str()))?;
        let run = service
            .run
            .ok_or_else(|| anyhow!("service '{}' missing run command", service_name.as_str()))?;
        let depends_on = service
            .depends_on
            .unwrap_or_default()
            .into_iter()
            .map(|dependency| {
                dependency
                    .parse()
                    .map_err(|message: String| anyhow!(message))
            })
            .collect::<Result<Vec<ServiceName>>>()?;

        services.insert(
            service_name,
            ResolvedServiceConfig {
                workdir: interpolate_template(&workdir, &vars),
                run: interpolate_template(&run, &vars),
                bootstrap: service
                    .bootstrap
                    .map(|command| interpolate_template(&command, &vars)),
                stop: service
                    .stop
                    .map(|command| interpolate_template(&command, &vars)),
                depends_on,
            },
        );
    }

    let database = resolved_layer
        .database
        .map(|database| {
            Ok::<_, anyhow::Error>(ResolvedDatabaseConfig {
                backup: interpolate_template(
                    &database
                        .backup
                        .ok_or_else(|| anyhow!("database backup command is required"))?,
                    &vars,
                ),
                restore: interpolate_template(
                    &database
                        .restore
                        .ok_or_else(|| anyhow!("database restore command is required"))?,
                    &vars,
                ),
                verify: database
                    .verify
                    .map(|command| interpolate_template(&command, &vars)),
            })
        })
        .transpose()?;

    let deployment = resolved_layer
        .deployment
        .map(|deployment| {
            let build = deployment
                .build
                .ok_or_else(|| anyhow!("deployment build commands are required"))?
                .into_iter()
                .map(|command| interpolate_template(&command, &vars))
                .collect::<Vec<_>>();
            if build.is_empty() {
                bail!("deployment requires at least one build command");
            }
            let migrate = interpolate_template(
                &deployment
                    .migrate
                    .ok_or_else(|| anyhow!("deployment migration command is required"))?,
                &vars,
            );
            let artifacts = deployment
                .artifacts
                .into_iter()
                .map(|(name, path)| (name, interpolate_template(&path, &vars)))
                .collect();
            let run = deployment
                .run
                .into_iter()
                .map(|(name, command)| {
                    let service = name.parse().map_err(|message: String| anyhow!(message))?;
                    Ok((service, interpolate_template(&command, &vars)))
                })
                .collect::<Result<BTreeMap<_, _>>>()?;
            let retention = deployment.retention.unwrap_or(5);
            if retention < 2 {
                bail!("deployment retention must keep at least two releases");
            }
            Ok::<_, anyhow::Error>(ResolvedDeploymentConfig {
                build,
                migrate,
                artifacts,
                run,
                retention,
            })
        })
        .transpose()?;

    Ok(ResolvedProfile {
        profile_name: profile_name.to_owned(),
        state_namespace,
        vars,
        env,
        services,
        database,
        deployment,
    })
}

fn pid_path(paths: &Paths, service: ServiceName) -> PathBuf {
    paths.pid_dir.join(format!("{}.pid", service.as_str()))
}

fn log_path(paths: &Paths, service: ServiceName) -> PathBuf {
    paths.log_dir.join(format!("{}.log", service.as_str()))
}

fn pid_running(pid: i32) -> bool {
    if kill(Pid::from_raw(pid), None).is_err() {
        return false;
    }
    let Ok(stat) = fs::read_to_string(format!("/proc/{pid}/stat")) else {
        return true;
    };
    stat.rsplit_once(") ")
        .and_then(|(_, fields)| fields.chars().next())
        != Some('Z')
}

fn read_pids(paths: &Paths, service: ServiceName) -> Result<Option<ServicePids>> {
    let path = pid_path(paths, service);
    if !path.exists() {
        return Ok(None);
    }

    let raw = fs::read_to_string(&path)?;
    let mut service_pid = None;
    for line in raw.lines() {
        if let Some(value) = line.strip_prefix("service_pid=") {
            service_pid = Some(value.parse::<i32>()?);
        }
    }

    match service_pid {
        Some(service_pid) => Ok(Some(ServicePids { service_pid })),
        _ => Ok(None),
    }
}

fn write_pids(paths: &Paths, service: ServiceName, pids: &ServicePids) -> Result<()> {
    fs::write(
        pid_path(paths, service),
        format!("service_pid={}\n", pids.service_pid),
    )?;
    Ok(())
}

fn service_status(paths: &Paths, service: ServiceName) -> Result<bool> {
    let Some(pids) = read_pids(paths, service)? else {
        return Ok(false);
    };

    if pid_running(pids.service_pid) {
        return Ok(true);
    }

    let _ = fs::remove_file(pid_path(paths, service));
    Ok(false)
}

fn expected_service_port(profile: &ResolvedProfile, service: ServiceName) -> Option<u16> {
    let key = match service {
        ServiceName::Backend => "backend_port",
        ServiceName::Embedding => "embedding_port",
        ServiceName::Frontend => "frontend_port",
        ServiceName::Db => return None,
    };

    profile.vars.get(key)?.parse().ok()
}

fn frontend_url(profile: &ResolvedProfile) -> Option<String> {
    expected_service_port(profile, ServiceName::Frontend)
        .map(|port| format!("http://127.0.0.1:{port}/"))
}

fn print_profile_endpoints(profile: &ResolvedProfile) {
    eprintln!("\n=== profile endpoints ===");
    eprintln!("profile: {}", profile.profile_name);

    if let Some(port) = expected_service_port(profile, ServiceName::Frontend)
        && let Some(url) = frontend_url(profile)
    {
        eprintln!("frontend:  {url} (port {port})");
    }
    if let Some(port) = expected_service_port(profile, ServiceName::Backend) {
        eprintln!("backend:   http://127.0.0.1:{port} (port {port})");
    }
    if let Some(port) = expected_service_port(profile, ServiceName::Embedding) {
        eprintln!("embedding: http://127.0.0.1:{port} (port {port})");
    }
    if let Some(db_name) = profile.vars.get("db_name") {
        eprintln!("database:  {db_name}");
    }
}

fn service_endpoint(profile: &ResolvedProfile, service: ServiceName) -> String {
    match service {
        ServiceName::Db => profile
            .vars
            .get("db_name")
            .cloned()
            .unwrap_or_else(|| "-".to_owned()),
        ServiceName::Frontend => expected_service_port(profile, service)
            .map(|port| format!("http://127.0.0.1:{port}/"))
            .unwrap_or_else(|| "-".to_owned()),
        ServiceName::Backend | ServiceName::Embedding => expected_service_port(profile, service)
            .map(|port| format!("http://127.0.0.1:{port}"))
            .unwrap_or_else(|| "-".to_owned()),
    }
}

fn collect_service_status(
    paths: &Paths,
    profile: &ResolvedProfile,
    service: ServiceName,
) -> ServiceStatusReport {
    let (process, pid) = match read_pids(paths, service) {
        Ok(Some(pids)) if pid_running(pids.service_pid) => ("running", Some(pids.service_pid)),
        Ok(Some(pids)) => ("stopped", Some(pids.service_pid)),
        Ok(None) => ("stopped", None),
        Err(_) => ("error", None),
    };
    let readiness = match service_ready(paths, profile, service) {
        Ok(true) => "ready",
        Ok(false) => "not-ready",
        Err(_) => "error",
    };

    ServiceStatusReport {
        service,
        process,
        pid,
        readiness,
        endpoint: service_endpoint(profile, service),
        log_path: log_path(paths, service),
    }
}

fn classify_status(reports: &[ServiceStatusReport], migrations_healthy: bool) -> OverallStatus {
    if reports.is_empty() {
        OverallStatus::Stopped
    } else if reports
        .iter()
        .all(|report| report.process == "running" && report.readiness == "ready")
        && migrations_healthy
    {
        OverallStatus::Healthy
    } else if reports
        .iter()
        .all(|report| report.process == "stopped" && report.readiness == "not-ready")
    {
        OverallStatus::Stopped
    } else {
        OverallStatus::Degraded
    }
}

fn local_migration_versions(paths: &Paths) -> Result<BTreeSet<i64>> {
    let mut versions = BTreeSet::new();
    for entry in fs::read_dir(paths.repo_root.join("migrations"))? {
        let entry = entry?;
        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();
        if entry.path().extension().and_then(|value| value.to_str()) != Some("sql") {
            continue;
        }
        let Some(version) = file_name.split('_').next() else {
            continue;
        };
        versions.insert(
            version
                .parse()
                .with_context(|| format!("invalid migration file name '{file_name}'"))?,
        );
    }
    Ok(versions)
}

fn migration_status(paths: &Paths, profile: &ResolvedProfile) -> (String, bool) {
    if !profile.services.contains_key(&ServiceName::Db) {
        return ("not checked (external database)".to_owned(), true);
    }
    let Some(db_name) = profile.vars.get("db_name") else {
        return ("unknown (database name missing)".to_owned(), false);
    };
    let Ok(local) = local_migration_versions(paths) else {
        return ("unknown (cannot read local migrations)".to_owned(), false);
    };
    let query = "SELECT version || ':' || success FROM _sqlx_migrations ORDER BY version";
    let command = format!(
        "docker compose exec -T postgres psql -U postgres -d {} -tAc {}",
        shell_quote(db_name),
        shell_quote(query)
    );
    let Ok(output) = shell_command(&command, &paths.repo_root, &profile.env).output() else {
        return ("unknown (migration query failed)".to_owned(), false);
    };
    if !output.status.success() {
        return ("unknown (migration query failed)".to_owned(), false);
    }

    let mut applied = BTreeSet::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let Some((version, success)) = line.trim().split_once(':') else {
            return ("unknown (unexpected migration data)".to_owned(), false);
        };
        if success != "true" && success != "t" {
            return (format!("failed migration {version}"), false);
        }
        let Ok(version) = version.parse() else {
            return ("unknown (unexpected migration data)".to_owned(), false);
        };
        applied.insert(version);
    }

    if applied == local {
        (format!("current ({} applied)", applied.len()), true)
    } else if applied.is_subset(&local) {
        (
            format!("{} pending", local.difference(&applied).count()),
            false,
        )
    } else {
        ("diverged from local migrations".to_owned(), false)
    }
}

fn format_bytes(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = KIB * 1024;
    const GIB: u64 = MIB * 1024;
    if bytes >= GIB {
        format!("{:.1} GiB", bytes as f64 / GIB as f64)
    } else if bytes >= MIB {
        format!("{:.1} MiB", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.1} KiB", bytes as f64 / KIB as f64)
    } else {
        format!("{bytes} B")
    }
}

fn backup_metadata_path(backup_path: &Path) -> PathBuf {
    let mut file_name = backup_path.file_name().unwrap_or_default().to_os_string();
    file_name.push(".json");
    backup_path.with_file_name(file_name)
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn read_backup_metadata(backup_path: &Path) -> Result<Option<BackupMetadata>> {
    let path = backup_metadata_path(backup_path);
    if !path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(&path)
        .with_context(|| format!("failed to read backup metadata {}", path.display()))?;
    serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse backup metadata {}", path.display()))
        .map(Some)
}

fn write_backup_metadata(backup_path: &Path, metadata: &BackupMetadata) -> Result<()> {
    let path = backup_metadata_path(backup_path);
    let temporary = path.with_extension("json.tmp");
    let mut raw = serde_json::to_string_pretty(metadata)?;
    raw.push('\n');
    fs::write(&temporary, raw)?;
    fs::rename(&temporary, &path)?;
    Ok(())
}

fn backup_verification_label(backup_path: &Path) -> String {
    let metadata = match read_backup_metadata(backup_path) {
        Ok(Some(metadata)) => metadata,
        Ok(None) => return "unverified (no metadata)".to_owned(),
        Err(_) => return "invalid metadata".to_owned(),
    };
    let checksum = match sha256_file(backup_path) {
        Ok(checksum) => checksum,
        Err(_) => return "unreadable".to_owned(),
    };
    if checksum != metadata.sha256 {
        return "CORRUPT (checksum mismatch)".to_owned();
    }
    if metadata.verification.status == "verified" {
        metadata
            .verification
            .verified_at
            .map(|value| format!("verified {value}"))
            .unwrap_or_else(|| "verified".to_owned())
    } else {
        metadata.verification.status
    }
}

fn latest_backup(paths: &Paths) -> (String, bool) {
    let Ok(entries) = fs::read_dir(&paths.backup_dir) else {
        return ("none".to_owned(), true);
    };
    let latest = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            if entry.path().extension().and_then(|value| value.to_str()) != Some("sql") {
                return None;
            }
            let metadata = entry.metadata().ok()?;
            if !metadata.is_file() {
                return None;
            }
            Some((metadata.modified().ok()?, entry, metadata.len()))
        })
        .max_by_key(|(modified, _, _)| *modified);

    let Some((modified, entry, size)) = latest else {
        return ("none".to_owned(), true);
    };
    let modified = chrono::DateTime::<Utc>::from(modified)
        .format("%Y-%m-%d %H:%M:%S UTC")
        .to_string();
    let verification = backup_verification_label(&entry.path());
    let healthy = verification.starts_with("verified");
    (
        format!(
            "{} ({}, {modified}, {verification})",
            entry.file_name().to_string_lossy(),
            format_bytes(size),
        ),
        healthy,
    )
}

fn show_status(paths: &Paths, profile: &ResolvedProfile) -> OverallStatus {
    let reports = SERVICE_ORDER
        .iter()
        .copied()
        .filter(|service| profile.services.contains_key(service))
        .map(|service| collect_service_status(paths, profile, service))
        .collect::<Vec<_>>();
    let (migrations, migrations_healthy) = migration_status(paths, profile);
    let (latest_backup, backup_healthy) = latest_backup(paths);
    let overall = classify_status(&reports, migrations_healthy && backup_healthy);

    println!("Profile: {}", profile.profile_name);
    println!("State:   {}", paths.state_dir.display());
    println!();
    println!(
        "{:<10} {:<9} {:<10} {:<8} Endpoint",
        "Service", "Process", "Readiness", "PID"
    );
    for report in &reports {
        let pid = report
            .pid
            .map(|value| value.to_string())
            .unwrap_or_else(|| "-".to_owned());
        println!(
            "{:<10} {:<9} {:<10} {:<8} {}",
            report.service.as_str(),
            report.process,
            report.readiness,
            pid,
            report.endpoint
        );
        println!("           log: {}", report.log_path.display());
    }
    println!();
    println!("Migrations: {migrations}");
    println!("Latest backup: {latest_backup}");
    println!("Overall: {}", overall.as_str());

    overall
}

fn check_result(
    results: &mut Vec<CheckResult>,
    name: impl Into<String>,
    outcome: CheckOutcome,
    detail: impl Into<String>,
) {
    results.push(CheckResult {
        name: name.into(),
        outcome,
        detail: detail.into(),
    });
}

fn check_http_response(
    client: &reqwest::blocking::Client,
    url: &str,
) -> Result<reqwest::blocking::Response> {
    let response = client.get(url).send()?;
    if !response.status().is_success() {
        bail!("HTTP {}", response.status());
    }
    Ok(response)
}

fn check_api(
    client: &reqwest::blocking::Client,
    profile: &ResolvedProfile,
    results: &mut Vec<CheckResult>,
) {
    let Some(backend_port) = expected_service_port(profile, ServiceName::Backend) else {
        check_result(
            results,
            "backend API",
            CheckOutcome::Fail,
            "backend_port is not configured",
        );
        return;
    };
    let base_url = format!("http://127.0.0.1:{backend_port}");

    match check_http_response(client, &format!("{base_url}/health")) {
        Ok(response) => match response.text() {
            Ok(body) if body.trim() == "ok" => check_result(
                results,
                "backend health",
                CheckOutcome::Pass,
                "/health returned ok",
            ),
            Ok(body) => check_result(
                results,
                "backend health",
                CheckOutcome::Fail,
                format!("unexpected response body: {body:?}"),
            ),
            Err(error) => check_result(
                results,
                "backend health",
                CheckOutcome::Fail,
                error.to_string(),
            ),
        },
        Err(error) => check_result(
            results,
            "backend health",
            CheckOutcome::Fail,
            error.to_string(),
        ),
    }

    let settings_url = format!("{base_url}/api/site-settings");
    match check_http_response(client, &settings_url)
        .and_then(|response| response.json::<serde_json::Value>().map_err(Into::into))
    {
        Ok(settings) if settings.is_object() => check_result(
            results,
            "site settings",
            CheckOutcome::Pass,
            "returned a JSON object",
        ),
        Ok(_) => check_result(
            results,
            "site settings",
            CheckOutcome::Fail,
            "response is not a JSON object",
        ),
        Err(error) => check_result(
            results,
            "site settings",
            CheckOutcome::Fail,
            error.to_string(),
        ),
    }

    let documents_url = format!("{base_url}/api/documents");
    let documents = match check_http_response(client, &documents_url)
        .and_then(|response| response.json::<serde_json::Value>().map_err(Into::into))
    {
        Ok(documents) if documents.is_array() => {
            let count = documents.as_array().map_or(0, Vec::len);
            check_result(
                results,
                "document list",
                CheckOutcome::Pass,
                format!("returned {count} visible document(s)"),
            );
            documents
        }
        Ok(_) => {
            check_result(
                results,
                "document list",
                CheckOutcome::Fail,
                "response is not a JSON array",
            );
            serde_json::Value::Null
        }
        Err(error) => {
            check_result(
                results,
                "document list",
                CheckOutcome::Fail,
                error.to_string(),
            );
            serde_json::Value::Null
        }
    };

    let first_document_id = documents
        .as_array()
        .and_then(|documents| documents.first())
        .and_then(|document| document.get("id"))
        .and_then(serde_json::Value::as_str);
    if let Some(document_id) = first_document_id {
        let detail_url = format!("{base_url}/api/documents/{document_id}");
        match check_http_response(client, &detail_url)
            .and_then(|response| response.json::<serde_json::Value>().map_err(Into::into))
        {
            Ok(detail)
                if detail.get("document").is_some()
                    && detail.get("sections").is_some_and(|value| value.is_array()) =>
            {
                check_result(
                    results,
                    "document detail",
                    CheckOutcome::Pass,
                    "returned document and sections",
                )
            }
            Ok(_) => check_result(
                results,
                "document detail",
                CheckOutcome::Fail,
                "response is missing document or sections",
            ),
            Err(error) => check_result(
                results,
                "document detail",
                CheckOutcome::Fail,
                error.to_string(),
            ),
        }
    } else if documents.is_array() {
        check_result(
            results,
            "document detail",
            CheckOutcome::Skip,
            "no visible document available",
        );
    }

    check_authentication(client, &base_url, results);
}

fn check_credential(name: &str) -> Option<String> {
    env::var(name).ok().filter(|value| !value.is_empty())
}

fn check_authentication(
    client: &reqwest::blocking::Client,
    base_url: &str,
    results: &mut Vec<CheckResult>,
) {
    let username = check_credential("RIMBUN_CHECK_USERNAME");
    let password = check_credential("RIMBUN_CHECK_PASSWORD");
    let (username, password) = match (username, password) {
        (Some(username), Some(password)) => (username, password),
        (username, password) => {
            let outcome = if username.is_none() && password.is_none() {
                CheckOutcome::Skip
            } else {
                CheckOutcome::Fail
            };
            let detail = if outcome == CheckOutcome::Skip {
                "RIMBUN_CHECK_USERNAME and RIMBUN_CHECK_PASSWORD are not configured"
            } else {
                "both RIMBUN_CHECK_USERNAME and RIMBUN_CHECK_PASSWORD are required"
            };
            check_result(results, "authenticated API", outcome, detail);
            return;
        }
    };

    let login = client
        .post(format!("{base_url}/api/auth/login"))
        .json(&serde_json::json!({ "identifier": username, "password": password }))
        .send();
    let session_token = match login {
        Ok(response) if response.status().is_success() => {
            match response.json::<serde_json::Value>() {
                Ok(body) => body
                    .get("session_token")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned),
                Err(error) => {
                    check_result(
                        results,
                        "login",
                        CheckOutcome::Fail,
                        format!("invalid JSON response: {error}"),
                    );
                    None
                }
            }
        }
        Ok(response) => {
            check_result(
                results,
                "login",
                CheckOutcome::Fail,
                format!("HTTP {}", response.status()),
            );
            None
        }
        Err(error) => {
            check_result(results, "login", CheckOutcome::Fail, error.to_string());
            None
        }
    };
    let Some(session_token) = session_token else {
        if !results.iter().any(|result| result.name == "login") {
            check_result(
                results,
                "login",
                CheckOutcome::Fail,
                "response did not contain session_token",
            );
        }
        return;
    };
    check_result(results, "login", CheckOutcome::Pass, "credentials accepted");

    match client
        .get(format!("{base_url}/api/me"))
        .header("x-rimbun-session", &session_token)
        .send()
    {
        Ok(response) if response.status().is_success() => check_result(
            results,
            "authenticated user",
            CheckOutcome::Pass,
            "/api/me accepted the session",
        ),
        Ok(response) => check_result(
            results,
            "authenticated user",
            CheckOutcome::Fail,
            format!("HTTP {}", response.status()),
        ),
        Err(error) => check_result(
            results,
            "authenticated user",
            CheckOutcome::Fail,
            error.to_string(),
        ),
    }

    match client
        .post(format!("{base_url}/api/auth/logout"))
        .header("x-rimbun-session", &session_token)
        .send()
    {
        Ok(response) if response.status().is_success() => check_result(
            results,
            "logout",
            CheckOutcome::Pass,
            "smoke-test session removed",
        ),
        Ok(response) => check_result(
            results,
            "logout",
            CheckOutcome::Fail,
            format!("HTTP {}", response.status()),
        ),
        Err(error) => check_result(results, "logout", CheckOutcome::Fail, error.to_string()),
    }
}

fn run_checks(paths: &Paths, profile: &ResolvedProfile) -> bool {
    let mut results = Vec::new();
    for service in SERVICE_ORDER
        .iter()
        .copied()
        .filter(|service| profile.services.contains_key(service))
    {
        let report = collect_service_status(paths, profile, service);
        let healthy = report.process == "running" && report.readiness == "ready";
        check_result(
            &mut results,
            format!("{} service", service.as_str()),
            if healthy {
                CheckOutcome::Pass
            } else {
                CheckOutcome::Fail
            },
            format!("process={}, readiness={}", report.process, report.readiness),
        );
    }

    let (migration_detail, migrations_healthy) = migration_status(paths, profile);
    let migration_outcome = if migration_detail.starts_with("not checked") {
        CheckOutcome::Skip
    } else if migrations_healthy {
        CheckOutcome::Pass
    } else {
        CheckOutcome::Fail
    };
    check_result(
        &mut results,
        "database migrations",
        migration_outcome,
        migration_detail,
    );

    let (backup_detail, backup_healthy) = latest_backup(paths);
    let backup_outcome = if backup_detail == "none" {
        CheckOutcome::Skip
    } else if backup_healthy {
        CheckOutcome::Pass
    } else {
        CheckOutcome::Fail
    };
    check_result(&mut results, "latest backup", backup_outcome, backup_detail);

    if let Some(frontend_url) = frontend_url(profile) {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(5))
            .build();
        match client {
            Ok(client) => {
                match check_http_response(&client, &frontend_url)
                    .and_then(|response| response.text().map_err(Into::into))
                {
                    Ok(body) if body.to_ascii_lowercase().contains("<!doctype html") => {
                        check_result(
                            &mut results,
                            "frontend page",
                            CheckOutcome::Pass,
                            frontend_url,
                        )
                    }
                    Ok(_) => check_result(
                        &mut results,
                        "frontend page",
                        CheckOutcome::Fail,
                        "response is not an HTML document",
                    ),
                    Err(error) => check_result(
                        &mut results,
                        "frontend page",
                        CheckOutcome::Fail,
                        error.to_string(),
                    ),
                }
                check_api(&client, profile, &mut results);
            }
            Err(error) => check_result(
                &mut results,
                "HTTP client",
                CheckOutcome::Fail,
                error.to_string(),
            ),
        }
    } else {
        check_result(
            &mut results,
            "frontend page",
            CheckOutcome::Skip,
            "frontend is not configured",
        );
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(5))
            .build();
        match client {
            Ok(client) => check_api(&client, profile, &mut results),
            Err(error) => check_result(
                &mut results,
                "HTTP client",
                CheckOutcome::Fail,
                error.to_string(),
            ),
        }
    }

    println!("Profile check: {}\n", profile.profile_name);
    for result in &results {
        let label = match result.outcome {
            CheckOutcome::Pass => "PASS",
            CheckOutcome::Fail => "FAIL",
            CheckOutcome::Skip => "SKIP",
        };
        println!("[{label}] {:<24} {}", result.name, result.detail);
    }
    let passed = results
        .iter()
        .filter(|result| result.outcome == CheckOutcome::Pass)
        .count();
    let failed = results
        .iter()
        .filter(|result| result.outcome == CheckOutcome::Fail)
        .count();
    let skipped = results
        .iter()
        .filter(|result| result.outcome == CheckOutcome::Skip)
        .count();
    println!("\nResult: {passed} passed, {failed} failed, {skipped} skipped");

    checks_succeeded(&results)
}

fn checks_succeeded(results: &[CheckResult]) -> bool {
    !results
        .iter()
        .any(|result| result.outcome == CheckOutcome::Fail)
}

fn local_port_in_use(port: u16) -> Result<bool> {
    match TcpListener::bind(("127.0.0.1", port)) {
        Ok(listener) => {
            drop(listener);
            Ok(false)
        }
        Err(error) if error.kind() == ErrorKind::AddrInUse => Ok(true),
        Err(error) => Err(error.into()),
    }
}

fn port_usage_details(port: u16) -> Option<String> {
    let output = Command::new("ss")
        .args(["-ltnp", &format!("sport = :{port}")])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if stdout.is_empty() {
        None
    } else {
        Some(stdout)
    }
}

fn profiles_using_service_port(
    registry: &ConfigRegistry,
    current_profile: &ResolvedProfile,
    service: ServiceName,
    port: u16,
) -> Vec<String> {
    let mut matches = Vec::new();

    for profile_name in registry.profiles.keys() {
        if profile_name == &current_profile.profile_name {
            continue;
        }

        let Ok(profile) = resolve_profile(registry, profile_name) else {
            continue;
        };

        if expected_service_port(&profile, service) != Some(port) {
            continue;
        }

        let Ok(paths) = state_paths(&profile.state_namespace) else {
            continue;
        };

        if service_status(&paths, service).unwrap_or(false) {
            matches.push(profile.profile_name);
        }
    }

    matches
}

fn shell_command(command: &str, workdir: &Path, env: &BTreeMap<String, String>) -> Command {
    let mut cmd = Command::new("bash");
    cmd.arg("-lc").arg(command).current_dir(workdir);
    cmd.envs(env);
    cmd.process_group(0);
    cmd
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn run_shell(command: &str, workdir: &Path, env: &BTreeMap<String, String>) -> Result<()> {
    let status = shell_command(command, workdir, env).status()?;
    if !status.success() {
        bail!("command failed: {command}");
    }
    Ok(())
}

fn ensure_profile_database(paths: &Paths, profile: &ResolvedProfile) -> Result<()> {
    if !profile.services.contains_key(&ServiceName::Db) {
        return Ok(());
    }

    let Some(db_name) = profile.vars.get("db_name") else {
        return Ok(());
    };

    let exists_command = format!(
        "docker compose exec -T postgres psql -U postgres -d postgres -tAc {}",
        shell_quote(&format!(
            "SELECT 1 FROM pg_database WHERE datname = '{}'",
            db_name.replace('\'', "''")
        ))
    );
    let output = shell_command(&exists_command, &paths.repo_root, &profile.env).output()?;
    if output.status.success() && String::from_utf8_lossy(&output.stdout).trim() == "1" {
        return Ok(());
    }

    let create_command = format!("docker compose exec -T postgres createdb -U postgres {db_name}");
    run_shell(&create_command, &paths.repo_root, &profile.env)
}

fn start_logged_command(
    paths: &Paths,
    profile: &ResolvedProfile,
    service: ServiceName,
    workdir: &Path,
    command: &str,
) -> Result<()> {
    let log_path = log_path(paths, service);
    let log_writer = fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&log_path)?;
    let log_writer_err = log_writer.try_clone()?;

    let service_child = shell_command(command, workdir, &profile.env)
        .stdout(Stdio::from(log_writer))
        .stderr(Stdio::from(log_writer_err))
        .spawn()?;

    write_pids(
        paths,
        service,
        &ServicePids {
            service_pid: service_child.id() as i32,
        },
    )?;
    println!(
        "Started {} process for profile {}",
        service.as_str(),
        profile.profile_name
    );
    Ok(())
}

fn http_service_ready(port: u16, path: &str) -> bool {
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let Ok(mut stream) = TcpStream::connect_timeout(&address, Duration::from_millis(500)) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(500)));

    let request =
        format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
    if stream.write_all(request.as_bytes()).is_err() {
        return false;
    }

    let mut status_line = String::new();
    if BufReader::new(stream).read_line(&mut status_line).is_err() {
        return false;
    }

    matches!(
        status_line.split_whitespace().nth(1),
        Some(code) if code.starts_with('2')
    )
}

fn service_ready(paths: &Paths, profile: &ResolvedProfile, service: ServiceName) -> Result<bool> {
    match service {
        ServiceName::Db => {
            let output = shell_command(
                "docker compose exec -T postgres pg_isready -U postgres -d postgres",
                &paths.repo_root,
                &profile.env,
            )
            .output()?;
            Ok(output.status.success())
        }
        ServiceName::Embedding => Ok(expected_service_port(profile, service)
            .is_some_and(|port| http_service_ready(port, "/health"))),
        ServiceName::Backend => Ok(expected_service_port(profile, service)
            .is_some_and(|port| http_service_ready(port, "/health"))),
        ServiceName::Frontend => Ok(expected_service_port(profile, service)
            .is_some_and(|port| http_service_ready(port, "/"))),
    }
}

fn wait_for_service_ready(
    paths: &Paths,
    profile: &ResolvedProfile,
    service: ServiceName,
) -> Result<()> {
    println!("Waiting for {} to become ready...", service.as_str());
    let started_at = Instant::now();
    let mut next_report = READINESS_REPORT_INTERVAL;

    loop {
        if !service_status(paths, service)? {
            bail!(
                "'{}' exited before becoming ready; inspect {}",
                service.as_str(),
                log_path(paths, service).display()
            );
        }

        if service_ready(paths, profile, service)? {
            println!(
                "Ready: {} for profile {} ({:.1}s)",
                service.as_str(),
                profile.profile_name,
                started_at.elapsed().as_secs_f64()
            );
            return Ok(());
        }

        let elapsed = started_at.elapsed();
        if elapsed >= READINESS_TIMEOUT {
            bail!(
                "'{}' did not become ready within {} seconds; inspect {}",
                service.as_str(),
                READINESS_TIMEOUT.as_secs(),
                log_path(paths, service).display()
            );
        }
        if elapsed >= next_report {
            println!(
                "Still waiting for {} ({:.0}s elapsed)...",
                service.as_str(),
                elapsed.as_secs_f64()
            );
            next_report += READINESS_REPORT_INTERVAL;
        }

        thread::sleep(READINESS_POLL_INTERVAL);
    }
}

fn stop_logged_command(paths: &Paths, service: ServiceName) -> Result<()> {
    let Some(pids) = read_pids(paths, service)? else {
        return Ok(());
    };

    let _ = killpg(Pid::from_raw(pids.service_pid), Signal::SIGTERM);
    wait_for_pid_exit(pids.service_pid, Duration::from_secs(3));

    if pid_running(pids.service_pid) {
        let _ = killpg(Pid::from_raw(pids.service_pid), Signal::SIGKILL);
        wait_for_pid_exit(pids.service_pid, Duration::from_secs(1));
    }

    let _ = fs::remove_file(pid_path(paths, service));
    Ok(())
}

fn wait_for_pid_exit(pid: i32, timeout: Duration) {
    let sleep_step = Duration::from_millis(100);
    let mut waited = Duration::ZERO;
    while waited < timeout {
        if !pid_running(pid) {
            return;
        }
        thread::sleep(sleep_step);
        waited += sleep_step;
    }
}

fn service_config(
    profile: &ResolvedProfile,
    service: ServiceName,
) -> Result<&ResolvedServiceConfig> {
    profile.services.get(&service).ok_or_else(|| {
        anyhow!(
            "service '{}' missing from resolved profile",
            service.as_str()
        )
    })
}

fn database_config(profile: &ResolvedProfile) -> Result<&ResolvedDatabaseConfig> {
    profile
        .database
        .as_ref()
        .ok_or_else(|| anyhow!("profile has no database backup configuration"))
}

fn dependency_order(profile: &ResolvedProfile, target: &ServiceTarget) -> Result<Vec<ServiceName>> {
    let roots = match target {
        ServiceTarget::All => SERVICE_ORDER
            .iter()
            .copied()
            .filter(|service| profile.services.contains_key(service))
            .collect::<Vec<_>>(),
        ServiceTarget::One(service) => vec![*service],
    };

    let mut ordered = Vec::new();
    let mut visiting = BTreeSet::new();
    let mut visited = BTreeSet::new();

    fn visit(
        profile: &ResolvedProfile,
        service: ServiceName,
        ordered: &mut Vec<ServiceName>,
        visiting: &mut BTreeSet<ServiceName>,
        visited: &mut BTreeSet<ServiceName>,
    ) -> Result<()> {
        if visited.contains(&service) {
            return Ok(());
        }
        if !visiting.insert(service) {
            bail!("cyclic service dependency involving '{}'", service.as_str());
        }

        let config = service_config(profile, service)?;
        for dependency in &config.depends_on {
            visit(profile, *dependency, ordered, visiting, visited)?;
        }

        let _ = visiting.remove(&service);
        visited.insert(service);
        ordered.push(service);
        Ok(())
    }

    for service in roots {
        visit(profile, service, &mut ordered, &mut visiting, &mut visited)?;
    }

    Ok(ordered)
}

fn start_service(
    registry: &ConfigRegistry,
    paths: &Paths,
    profile: &ResolvedProfile,
    service: ServiceName,
) -> Result<()> {
    if service_status(paths, service)? {
        println!("{} already running", service.as_str());
        return wait_for_service_ready(paths, profile, service);
    }

    println!(
        "Starting {} for profile {}",
        service.as_str(),
        profile.profile_name
    );

    ensure_service_port_available(registry, profile, service)?;

    let config = service_config(profile, service)?;
    let workdir = paths.repo_root.join(&config.workdir);
    if let Some(bootstrap) = &config.bootstrap {
        run_shell(bootstrap, &workdir, &profile.env)?;
    }
    start_logged_command(paths, profile, service, &workdir, &config.run)?;
    wait_for_service_ready(paths, profile, service)
}

fn ensure_service_port_available(
    registry: &ConfigRegistry,
    profile: &ResolvedProfile,
    service: ServiceName,
) -> Result<()> {
    if let Some(port) = expected_service_port(profile, service)
        && local_port_in_use(port)?
    {
        let conflicting_profiles = profiles_using_service_port(registry, profile, service, port);
        let profile_hint = if conflicting_profiles.is_empty() {
            String::new()
        } else {
            format!(
                "\n\nLikely conflicting profile(s): {}",
                conflicting_profiles.join(", ")
            )
        };
        let details = port_usage_details(port)
            .map(|output| format!("\n\nPort usage:\n{output}"))
            .unwrap_or_default();
        bail!(
            "cannot start '{}' for profile '{}': port {} is already in use{}{}",
            service.as_str(),
            profile.profile_name,
            port,
            profile_hint,
            details
        );
    }
    Ok(())
}

fn stop_service(paths: &Paths, profile: &ResolvedProfile, service: ServiceName) -> Result<()> {
    stop_logged_command(paths, service)?;
    if let Some(command) = &service_config(profile, service)?.stop {
        let workdir = paths
            .repo_root
            .join(&service_config(profile, service)?.workdir);
        run_shell(command, &workdir, &profile.env)?;
    }
    println!(
        "Stopped {} for profile {}",
        service.as_str(),
        profile.profile_name
    );
    Ok(())
}

fn show_logs(paths: &Paths, target: &ServiceTarget, follow: bool) -> Result<()> {
    match target {
        ServiceTarget::All => {
            if follow {
                let files: Vec<PathBuf> = SERVICE_ORDER
                    .iter()
                    .map(|service| log_path(paths, *service))
                    .collect();
                let mut cmd = Command::new("tail");
                cmd.arg("-n").arg("200").arg("-f");
                for file in files {
                    cmd.arg(file);
                }
                cmd.status()?;
            } else {
                for service in SERVICE_ORDER {
                    println!("\n== {} ==", service.as_str());
                    let file = log_path(paths, service);
                    if file.exists() {
                        Command::new("tail")
                            .arg("-n")
                            .arg("80")
                            .arg(file)
                            .status()?;
                    }
                }
            }
        }
        ServiceTarget::One(service) => {
            let file = log_path(paths, *service);
            if !file.exists() {
                bail!("no log file for service '{}'", service.as_str());
            }
            let mut cmd = Command::new("tail");
            if follow {
                cmd.arg("-n").arg("200").arg("-f");
            } else {
                cmd.arg("-n").arg("80");
            }
            cmd.arg(file).status()?;
        }
    }

    Ok(())
}

fn set_password(
    paths: &Paths,
    profile: &ResolvedProfile,
    username: &str,
    new_password: &str,
) -> Result<()> {
    if username.is_empty() {
        bail!("username is required");
    }
    if new_password.is_empty() {
        bail!("new password is required");
    }

    let status = Command::new("cargo")
        .arg("run")
        .arg("-p")
        .arg("rimbun-api")
        .arg("--bin")
        .arg("rimbun-set-password")
        .arg("--")
        .arg(username)
        .arg(new_password)
        .current_dir(&paths.repo_root)
        .envs(&profile.env)
        .status()?;

    if !status.success() {
        bail!("failed to set password");
    }
    Ok(())
}

fn list_users(paths: &Paths, profile: &ResolvedProfile) -> Result<()> {
    let status = Command::new("cargo")
        .arg("run")
        .arg("-p")
        .arg("rimbun-api")
        .arg("--bin")
        .arg("rimbun-list-users")
        .current_dir(&paths.repo_root)
        .envs(&profile.env)
        .status()?;

    if !status.success() {
        bail!("failed to list users");
    }
    Ok(())
}

fn export_contributions(
    registry: &ConfigRegistry,
    paths: &Paths,
    profile: &ResolvedProfile,
    username: &str,
    file: Option<&str>,
) -> Result<()> {
    if username.is_empty() {
        bail!("username is required");
    }

    ensure_db_running(registry, paths, profile)?;

    let mut command = Command::new("cargo");
    command
        .arg("run")
        .arg("-p")
        .arg("rimbun-api")
        .arg("--bin")
        .arg("rimbun-export-user-contributions")
        .arg("--")
        .arg(username)
        .current_dir(&paths.repo_root)
        .envs(&profile.env);

    if let Some(file) = file {
        command.arg(file);
    }

    let status = command.status()?;
    if !status.success() {
        bail!("failed to export contributions");
    }
    Ok(())
}

fn import_contributions(
    registry: &ConfigRegistry,
    paths: &Paths,
    profile: &ResolvedProfile,
    username: &str,
    file: &str,
    publish: bool,
) -> Result<()> {
    if username.is_empty() {
        bail!("username is required");
    }
    if file.is_empty() {
        bail!("file is required");
    }

    ensure_db_running(registry, paths, profile)?;

    let mut command = Command::new("cargo");
    command
        .arg("run")
        .arg("-p")
        .arg("rimbun-api")
        .arg("--bin")
        .arg("rimbun-import-user-contributions")
        .arg("--")
        .arg(username)
        .arg(file)
        .current_dir(&paths.repo_root)
        .envs(&profile.env);
    if publish {
        command.arg("--publish");
    }

    let status = command.status()?;

    if !status.success() {
        bail!("failed to import contributions");
    }
    Ok(())
}

fn set_role(paths: &Paths, profile: &ResolvedProfile, username: &str, role: &str) -> Result<()> {
    if username.is_empty() {
        bail!("username is required");
    }
    if role.is_empty() {
        bail!("role is required");
    }

    let status = Command::new("cargo")
        .arg("run")
        .arg("-p")
        .arg("rimbun-api")
        .arg("--bin")
        .arg("rimbun-set-role")
        .arg("--")
        .arg(username)
        .arg(role)
        .current_dir(&paths.repo_root)
        .envs(&profile.env)
        .status()?;

    if !status.success() {
        bail!("failed to set role");
    }
    Ok(())
}

fn ensure_db_running(
    registry: &ConfigRegistry,
    paths: &Paths,
    profile: &ResolvedProfile,
) -> Result<()> {
    if !profile.services.contains_key(&ServiceName::Db) {
        return Ok(());
    }

    if service_status(paths, ServiceName::Db)? {
        return Ok(());
    }

    start_service(registry, paths, profile, ServiceName::Db)
}

fn ensure_restore_safe(paths: &Paths) -> Result<()> {
    for service in [
        ServiceName::Frontend,
        ServiceName::Backend,
        ServiceName::Embedding,
    ] {
        if service_status(paths, service)? {
            bail!(
                "service '{}' is running; stop application services before restore",
                service.as_str()
            );
        }
    }
    Ok(())
}

fn sanitize_backup_name(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect();
    sanitized.trim_matches('_').to_owned()
}

fn verification_database_name(profile: &ResolvedProfile) -> String {
    let base = profile
        .vars
        .get("db_name")
        .map(String::as_str)
        .unwrap_or("rimbun");
    let nonce = Utc::now().timestamp_micros().unsigned_abs();
    let suffix = format!("_verify_{nonce}");
    let maximum_base_length = 63_usize.saturating_sub(suffix.len());
    let base = base
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || *character == '_')
        .take(maximum_base_length)
        .collect::<String>();
    format!("{base}{suffix}")
}

fn backup_created_at(backup_path: &Path) -> String {
    fs::metadata(backup_path)
        .and_then(|metadata| metadata.modified())
        .map(chrono::DateTime::<Utc>::from)
        .unwrap_or_else(|_| Utc::now())
        .to_rfc3339()
}

fn new_backup_metadata(profile: &ResolvedProfile, backup_path: &Path) -> Result<BackupMetadata> {
    Ok(BackupMetadata {
        format_version: 1,
        profile: profile.profile_name.clone(),
        database: profile.vars.get("db_name").cloned().unwrap_or_default(),
        created_at: backup_created_at(backup_path),
        size_bytes: fs::metadata(backup_path)?.len(),
        sha256: sha256_file(backup_path)?,
        verification: BackupVerification {
            status: "pending".to_owned(),
            verified_at: None,
        },
    })
}

fn verify_backup_file(paths: &Paths, profile: &ResolvedProfile, backup_path: &Path) -> Result<()> {
    if !backup_path.exists() {
        bail!("backup file '{}' not found", backup_path.display());
    }
    if fs::metadata(backup_path)?.len() == 0 {
        bail!("backup file '{}' is empty", backup_path.display());
    }

    let database = database_config(profile)?;
    let verify = database.verify.as_ref().ok_or_else(|| {
        anyhow!(
            "profile '{}' has no backup verification command",
            profile.profile_name
        )
    })?;
    let mut metadata = match read_backup_metadata(backup_path)? {
        Some(metadata) => {
            let checksum = sha256_file(backup_path)?;
            if checksum != metadata.sha256 {
                bail!(
                    "backup '{}' does not match its recorded SHA-256 checksum",
                    backup_path.display()
                );
            }
            metadata
        }
        None => new_backup_metadata(profile, backup_path)?,
    };
    metadata.verification.status = "verifying".to_owned();
    metadata.verification.verified_at = None;
    write_backup_metadata(backup_path, &metadata)?;

    let verification_database = verification_database_name(profile);
    let command = verify
        .replace("{file}", &shell_quote(&backup_path.display().to_string()))
        .replace("{verification_db}", &shell_quote(&verification_database));
    let verification_result = run_shell(&command, &paths.repo_root, &profile.env);
    match verification_result {
        Ok(()) => {
            metadata.verification.status = "verified".to_owned();
            metadata.verification.verified_at = Some(Utc::now().to_rfc3339());
            write_backup_metadata(backup_path, &metadata)?;
            println!(
                "Verified backup {} (SHA-256 {})",
                backup_path.display(),
                metadata.sha256
            );
            Ok(())
        }
        Err(error) => {
            metadata.verification.status = "failed".to_owned();
            metadata.verification.verified_at = None;
            write_backup_metadata(backup_path, &metadata)?;
            Err(error).context(format!(
                "backup verification failed for {}",
                backup_path.display()
            ))
        }
    }
}

fn create_backup(
    registry: &ConfigRegistry,
    paths: &Paths,
    profile: &ResolvedProfile,
    name: Option<&str>,
) -> Result<PathBuf> {
    ensure_db_running(registry, paths, profile)?;

    let database = database_config(profile)?;
    let timestamp = Utc::now().format("%Y%m%d-%H%M%S").to_string();
    let suffix = name
        .map(sanitize_backup_name)
        .filter(|value| !value.is_empty())
        .map(|value| format!("-{value}"))
        .unwrap_or_default();
    let backup_path = paths.backup_dir.join(format!("{timestamp}{suffix}.sql"));
    let command = database
        .backup
        .replace("{file}", &shell_quote(&backup_path.display().to_string()));

    run_shell(&command, &paths.repo_root, &profile.env)?;
    println!("Created backup {}", backup_path.display());
    let metadata = new_backup_metadata(profile, &backup_path)?;
    write_backup_metadata(&backup_path, &metadata)?;
    verify_backup_file(paths, profile, &backup_path)?;
    Ok(backup_path)
}

fn resolve_backup_path(paths: &Paths, backup: &str) -> PathBuf {
    let candidate = PathBuf::from(backup);
    if candidate.is_absolute() {
        candidate
    } else {
        paths.backup_dir.join(candidate)
    }
}

fn restore_backup(
    registry: &ConfigRegistry,
    paths: &Paths,
    profile: &ResolvedProfile,
    backup: &str,
    allow_profile_mismatch: bool,
) -> Result<()> {
    ensure_restore_safe(paths)?;
    ensure_db_running(registry, paths, profile)?;

    let backup_path = resolve_backup_path(paths, backup);
    if !backup_path.exists() {
        bail!("backup file '{}' not found", backup_path.display());
    }

    match read_backup_metadata(&backup_path)? {
        Some(metadata) => {
            let checksum = sha256_file(&backup_path)?;
            if checksum != metadata.sha256 {
                bail!(
                    "refusing to restore '{}': SHA-256 checksum mismatch",
                    backup_path.display()
                );
            }
            if metadata.verification.status != "verified" {
                bail!(
                    "refusing to restore '{}': verification status is '{}'; run verify-backup first",
                    backup_path.display(),
                    metadata.verification.status
                );
            }
            if metadata.profile != profile.profile_name && !allow_profile_mismatch {
                bail!(
                    "backup belongs to profile '{}', not '{}'; pass --allow-profile-mismatch to restore it intentionally",
                    metadata.profile,
                    profile.profile_name
                );
            }
        }
        None => eprintln!(
            "WARNING: restoring legacy backup without checksum or restore verification: {}",
            backup_path.display()
        ),
    }

    let database = database_config(profile)?;
    let command = database
        .restore
        .replace("{file}", &shell_quote(&backup_path.display().to_string()));
    run_shell(&command, &paths.repo_root, &profile.env)?;
    println!("Restored backup {}", backup_path.display());
    Ok(())
}

fn current_git_commit(paths: &Paths) -> Result<String> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&paths.repo_root)
        .output()?;
    if !output.status.success() {
        bail!("failed to determine current Git commit");
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

fn worktree_is_dirty(paths: &Paths) -> Result<bool> {
    let output = Command::new("git")
        .args(["status", "--porcelain", "--untracked-files=normal"])
        .current_dir(&paths.repo_root)
        .output()?;
    if !output.status.success() {
        bail!("failed to inspect Git worktree");
    }
    Ok(!output.stdout.is_empty())
}

fn deployment_config(profile: &ResolvedProfile) -> Result<&ResolvedDeploymentConfig> {
    profile.deployment.as_ref().ok_or_else(|| {
        anyhow!(
            "profile '{}' has no deployment configuration",
            profile.profile_name
        )
    })
}

fn deploy_preflight(paths: &Paths, profile: &ResolvedProfile, allow_dirty: bool) -> Result<()> {
    let deployment = deployment_config(profile)?;
    let database = database_config(profile)?;
    if database.verify.is_none() {
        bail!("deployment requires verified backup configuration");
    }
    if deployment
        .build
        .iter()
        .any(|command| command.trim().is_empty())
        || deployment.migrate.trim().is_empty()
    {
        bail!("deployment commands must not be empty");
    }
    for artifact in ["backend", "embedding", "migrate", "static", "frontend"] {
        if !deployment
            .artifacts
            .get(artifact)
            .is_some_and(|path| !path.trim().is_empty())
        {
            bail!("deployment artifact '{artifact}' is required");
        }
    }
    for service in [
        ServiceName::Embedding,
        ServiceName::Backend,
        ServiceName::Frontend,
    ] {
        if !profile.services.contains_key(&service) {
            bail!("deployment requires managed '{}' service", service.as_str());
        }
        if !deployment
            .run
            .get(&service)
            .is_some_and(|command| !command.trim().is_empty())
        {
            bail!(
                "deployment run command for '{}' is required",
                service.as_str()
            );
        }
    }
    if !allow_dirty && worktree_is_dirty(paths)? {
        bail!("Git worktree is dirty; commit changes or pass --allow-dirty intentionally");
    }
    Ok(())
}

fn release_path(paths: &Paths, release_id: &str) -> PathBuf {
    paths.release_dir.join(format!("{release_id}.json"))
}

fn release_artifact_dir(paths: &Paths, release_id: &str) -> PathBuf {
    paths.release_dir.join(release_id)
}

fn validate_release_id(release_id: &str) -> Result<()> {
    if release_id.is_empty()
        || !release_id
            .chars()
            .all(|character| character.is_ascii_digit() || character == '-')
    {
        bail!("invalid release id '{release_id}'");
    }
    Ok(())
}

fn copy_tree(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let target = destination.join(entry.file_name());
        if file_type.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else if file_type.is_file() {
            fs::copy(entry.path(), target)?;
        } else {
            bail!(
                "release artifacts must not contain symlinks or special files: {}",
                entry.path().display()
            );
        }
    }
    Ok(())
}

fn archive_release_artifacts(
    paths: &Paths,
    deployment: &ResolvedDeploymentConfig,
    release_id: &str,
) -> Result<PathBuf> {
    validate_release_id(release_id)?;
    let destination = release_artifact_dir(paths, release_id);
    let staging = paths.release_dir.join(format!(".{release_id}.staging"));
    if destination.exists() || staging.exists() {
        bail!("release artifact path already exists for '{release_id}'");
    }
    fs::create_dir_all(staging.join("bin"))?;

    let archive_result = (|| {
        for (name, target_name) in [
            ("backend", "rimbun-api"),
            ("embedding", "rimbun-embedding-service"),
            ("migrate", "rimbun-migrate"),
            ("static", "rimbun-static-server"),
        ] {
            let source = paths.repo_root.join(
                deployment
                    .artifacts
                    .get(name)
                    .ok_or_else(|| anyhow!("deployment artifact '{name}' is missing"))?,
            );
            if !source.is_file() {
                bail!("release artifact not found: {}", source.display());
            }
            fs::copy(source, staging.join("bin").join(target_name))?;
        }

        let frontend = paths.repo_root.join(
            deployment
                .artifacts
                .get("frontend")
                .ok_or_else(|| anyhow!("deployment artifact 'frontend' is missing"))?,
        );
        if !frontend.is_dir() {
            bail!(
                "frontend release artifact not found: {}",
                frontend.display()
            );
        }
        copy_tree(&frontend, &staging.join("web"))?;
        fs::rename(&staging, &destination)?;
        Ok(())
    })();

    if archive_result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    archive_result?;
    Ok(destination)
}

fn artifact_checksums(root: &Path) -> Result<BTreeMap<String, String>> {
    fn visit(root: &Path, current: &Path, checksums: &mut BTreeMap<String, String>) -> Result<()> {
        for entry in fs::read_dir(current)? {
            let entry = entry?;
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                visit(root, &entry.path(), checksums)?;
            } else if file_type.is_file() && entry.file_name() != "manifest.json" {
                let relative = entry
                    .path()
                    .strip_prefix(root)?
                    .to_string_lossy()
                    .to_string();
                checksums.insert(relative, sha256_file(&entry.path())?);
            }
        }
        Ok(())
    }

    let mut checksums = BTreeMap::new();
    visit(root, root, &mut checksums)?;
    Ok(checksums)
}

fn verify_release_artifacts(paths: &Paths, release: &ReleaseRecord) -> Result<()> {
    let root = release_artifact_dir(paths, &release.id);
    if release.artifact_checksums.is_empty() {
        eprintln!(
            "WARNING: release '{}' predates artifact checksums and cannot be integrity-verified",
            release.id
        );
        return Ok(());
    }
    let actual = artifact_checksums(&root)?;
    if actual != release.artifact_checksums {
        bail!(
            "release '{}' failed artifact integrity verification",
            release.id
        );
    }
    Ok(())
}

fn active_release_id(paths: &Paths) -> Result<Option<String>> {
    let current = paths.release_dir.join("current");
    match fs::read_link(&current) {
        Ok(target) => Ok(target
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_owned)),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("failed to read {}", current.display())),
    }
}

fn activate_release(paths: &Paths, release_id: &str) -> Result<()> {
    validate_release_id(release_id)?;
    let artifacts = release_artifact_dir(paths, release_id);
    if !artifacts.is_dir() {
        bail!("release '{release_id}' has no archived artifacts");
    }

    let current = paths.release_dir.join("current");
    if current.exists() && !current.is_symlink() {
        bail!("{} exists but is not a symlink", current.display());
    }
    let temporary = paths
        .release_dir
        .join(format!(".current-{}.tmp", std::process::id()));
    let _ = fs::remove_file(&temporary);
    symlink(release_id, &temporary)?;
    fs::rename(&temporary, &current)?;
    Ok(())
}

fn release_command(
    deployment: &ResolvedDeploymentConfig,
    service: ServiceName,
    artifact_dir: &Path,
) -> Result<String> {
    let template = deployment.run.get(&service).ok_or_else(|| {
        anyhow!(
            "deployment run command for '{}' is missing",
            service.as_str()
        )
    })?;
    Ok(template.replace(
        "{release_dir}",
        &shell_quote(&artifact_dir.display().to_string()),
    ))
}

fn start_release_service(
    registry: &ConfigRegistry,
    paths: &Paths,
    profile: &ResolvedProfile,
    service: ServiceName,
    release_id: &str,
) -> Result<()> {
    if service_status(paths, service)? {
        println!("{} already running", service.as_str());
        return wait_for_service_ready(paths, profile, service);
    }
    ensure_service_port_available(registry, profile, service)?;
    let deployment = deployment_config(profile)?;
    let artifacts = release_artifact_dir(paths, release_id);
    let command = release_command(deployment, service, &artifacts)?;
    println!("Starting {} from release {release_id}", service.as_str());
    start_logged_command(paths, profile, service, &paths.repo_root, &command)?;
    wait_for_service_ready(paths, profile, service)
}

fn start_release_services(
    registry: &ConfigRegistry,
    paths: &Paths,
    profile: &ResolvedProfile,
    release_id: &str,
) -> Result<()> {
    for service in [
        ServiceName::Embedding,
        ServiceName::Backend,
        ServiceName::Frontend,
    ] {
        start_release_service(registry, paths, profile, service, release_id)?;
    }
    Ok(())
}

fn start_configured_service(
    registry: &ConfigRegistry,
    paths: &Paths,
    profile: &ResolvedProfile,
    service: ServiceName,
    source: bool,
) -> Result<()> {
    if service == ServiceName::Db || source || profile.deployment.is_none() {
        return start_service(registry, paths, profile, service);
    }
    let Some(release_id) = active_release_id(paths)? else {
        return start_service(registry, paths, profile, service);
    };
    let _ = validate_rollback_target(paths, profile, &release_id)?;
    start_release_service(registry, paths, profile, service, &release_id)
}

fn stop_application_services(paths: &Paths, profile: &ResolvedProfile) -> Result<()> {
    for service in [
        ServiceName::Frontend,
        ServiceName::Backend,
        ServiceName::Embedding,
    ] {
        stop_service(paths, profile, service)?;
    }
    Ok(())
}

fn acquire_deployment_lock(paths: &Paths) -> Result<DeploymentLock> {
    let path = paths.state_dir.join("deploy.lock");
    if let Ok(raw) = fs::read_to_string(&path)
        && let Some(pid) = raw
            .lines()
            .find_map(|line| line.strip_prefix("pid="))
            .and_then(|value| value.parse::<i32>().ok())
    {
        if pid_running(pid) {
            bail!("another deployment is running for this profile (PID {pid})");
        }
        let _ = fs::remove_file(&path);
    }

    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .with_context(|| format!("failed to acquire deployment lock {}", path.display()))?;
    writeln!(file, "pid={}", std::process::id())?;
    writeln!(file, "created_at={}", Utc::now().to_rfc3339())?;
    Ok(DeploymentLock { path })
}

fn write_release(paths: &Paths, release: &ReleaseRecord) -> Result<()> {
    let path = release_path(paths, &release.id);
    let mut raw = serde_json::to_string_pretty(release)?;
    raw.push('\n');
    write_atomic(&path, raw.as_bytes())?;
    if release.artifact_dir.is_some() {
        let artifact_dir = release_artifact_dir(paths, &release.id);
        if artifact_dir.is_dir() {
            write_atomic(&artifact_dir.join("manifest.json"), raw.as_bytes())?;
        }
    }
    Ok(())
}

fn write_atomic(path: &Path, contents: &[u8]) -> Result<()> {
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, contents)?;
    fs::rename(&temporary, path)?;
    Ok(())
}

fn execute_release_phase<T>(
    paths: &Paths,
    release: &mut ReleaseRecord,
    name: &str,
    action: impl FnOnce() -> Result<T>,
) -> Result<T> {
    println!("\n=== deploy: {name} ===");
    match action() {
        Ok(value) => {
            release.phases.push(ReleasePhase {
                name: name.to_owned(),
                status: "passed".to_owned(),
                completed_at: Utc::now().to_rfc3339(),
                detail: None,
            });
            write_release(paths, release)?;
            Ok(value)
        }
        Err(error) => {
            release.phases.push(ReleasePhase {
                name: name.to_owned(),
                status: "failed".to_owned(),
                completed_at: Utc::now().to_rfc3339(),
                detail: Some(format!("{error:#}")),
            });
            release.status = "failed".to_owned();
            release.completed_at = Some(Utc::now().to_rfc3339());
            write_release(paths, release)?;
            Err(error).context(format!("deployment {} failed during '{name}'", release.id))
        }
    }
}

fn print_deployment_plan(
    paths: &Paths,
    profile: &ResolvedProfile,
    allow_dirty: bool,
) -> Result<()> {
    deploy_preflight(paths, profile, allow_dirty)?;
    let deployment = deployment_config(profile)?;
    println!("Deployment plan for profile {}", profile.profile_name);
    println!("Git commit: {}", current_git_commit(paths)?);
    println!("1. Ensure the managed database is ready");
    println!("2. Create and restore-verify a database backup");
    for (index, command) in deployment.build.iter().enumerate() {
        println!("{}. Build: {command}", index + 3);
    }
    let archive_step = deployment.build.len() + 3;
    println!("{archive_step}. Archive immutable release artifacts");
    let stop_step = archive_step + 1;
    println!("{stop_step}. Stop frontend, backend, and embedding services");
    println!("{}. Migrate: {}", stop_step + 1, deployment.migrate);
    println!("{}. Atomically activate the new release", stop_step + 2);
    println!(
        "{}. Start embedding, backend, and frontend services",
        stop_step + 3
    );
    println!("{}. Run profile smoke checks", stop_step + 4);
    println!(
        "{}. Retain the newest {} releases",
        stop_step + 5,
        deployment.retention
    );
    println!("\nDry run complete; no services or data were changed.");
    Ok(())
}

fn deploy_profile(
    registry: &ConfigRegistry,
    paths: &Paths,
    profile: &ResolvedProfile,
    allow_dirty: bool,
) -> Result<()> {
    let _lock = acquire_deployment_lock(paths)?;
    let release_id = Utc::now().format("%Y%m%d-%H%M%S-%3f").to_string();
    let worktree_dirty = worktree_is_dirty(paths)?;
    let mut release = ReleaseRecord {
        format_version: 2,
        id: release_id.clone(),
        profile: profile.profile_name.clone(),
        git_commit: current_git_commit(paths)?,
        worktree_dirty,
        started_at: Utc::now().to_rfc3339(),
        completed_at: None,
        status: "running".to_owned(),
        backup: None,
        artifact_dir: None,
        activated_at: None,
        artifact_checksums: BTreeMap::new(),
        phases: Vec::new(),
    };
    write_release(paths, &release)?;

    execute_release_phase(paths, &mut release, "preflight", || {
        deploy_preflight(paths, profile, allow_dirty)
    })?;
    execute_release_phase(paths, &mut release, "database readiness", || {
        ensure_db_running(registry, paths, profile)?;
        ensure_profile_database(paths, profile)
    })?;
    let backup_path = execute_release_phase(paths, &mut release, "verified backup", || {
        create_backup(
            registry,
            paths,
            profile,
            Some(&format!("deploy-{release_id}")),
        )
    })?;
    release.backup = Some(backup_path.display().to_string());
    write_release(paths, &release)?;

    let deployment = deployment_config(profile)?;
    execute_release_phase(paths, &mut release, "build", || {
        for command in &deployment.build {
            println!("Running: {command}");
            run_shell(command, &paths.repo_root, &profile.env)?;
        }
        Ok(())
    })?;
    release.artifact_dir = Some(release_id.clone());
    let checksums =
        execute_release_phase(paths, &mut release, "archive release artifacts", || {
            let artifacts = archive_release_artifacts(paths, deployment, &release_id)?;
            artifact_checksums(&artifacts)
        })?;
    release.artifact_checksums = checksums;
    write_release(paths, &release)?;
    execute_release_phase(paths, &mut release, "stop application services", || {
        stop_application_services(paths, profile)
    })?;
    execute_release_phase(paths, &mut release, "database migrations", || {
        let command = deployment.migrate.replace(
            "{release_dir}",
            &shell_quote(
                &release_artifact_dir(paths, &release_id)
                    .display()
                    .to_string(),
            ),
        );
        run_shell(&command, &paths.repo_root, &profile.env)
    })?;
    execute_release_phase(paths, &mut release, "activate release", || {
        activate_release(paths, &release_id)
    })?;
    release.activated_at = Some(Utc::now().to_rfc3339());
    write_release(paths, &release)?;
    execute_release_phase(paths, &mut release, "start application services", || {
        start_release_services(registry, paths, profile, &release_id)
    })?;
    execute_release_phase(paths, &mut release, "smoke checks", || {
        if run_checks(paths, profile) {
            Ok(())
        } else {
            bail!("profile smoke checks failed")
        }
    })?;

    release.status = "deployed".to_owned();
    release.completed_at = Some(Utc::now().to_rfc3339());
    write_release(paths, &release)?;
    prune_releases(paths, deployment.retention)?;
    println!("\nDeployment {release_id} completed successfully.");
    Ok(())
}

fn read_release(paths: &Paths, release_id: &str) -> Result<ReleaseRecord> {
    validate_release_id(release_id)?;
    let path = release_path(paths, release_id);
    let raw = fs::read_to_string(&path)
        .with_context(|| format!("failed to read release {}", path.display()))?;
    serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse release {}", path.display()))
}

fn load_releases(paths: &Paths) -> Result<Vec<ReleaseRecord>> {
    let mut releases = Vec::<ReleaseRecord>::new();
    for entry in fs::read_dir(&paths.release_dir)? {
        let entry = entry?;
        if entry.path().extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let raw = fs::read_to_string(entry.path())?;
        releases.push(serde_json::from_str(&raw).with_context(|| {
            format!("failed to parse release record {}", entry.path().display())
        })?);
    }
    releases.sort_by(|left, right| right.id.cmp(&left.id));
    Ok(releases)
}

fn prune_releases(paths: &Paths, retention: usize) -> Result<()> {
    let active = active_release_id(paths)?;
    let releases = load_releases(paths)?;
    let mut retained_artifacts = 0_usize;
    for release in releases {
        let artifacts = release_artifact_dir(paths, &release.id);
        if !artifacts.is_dir() {
            continue;
        }
        let protected = active.as_deref() == Some(&release.id) || retained_artifacts < retention;
        if protected {
            retained_artifacts += 1;
            continue;
        }
        fs::remove_dir_all(&artifacts)?;
        fs::remove_file(release_path(paths, &release.id))?;
        println!("Pruned release artifacts {}", release.id);
    }
    Ok(())
}

fn list_releases(paths: &Paths) -> Result<()> {
    let releases = load_releases(paths)?;
    if releases.is_empty() {
        println!("No deployments recorded for this profile.");
        return Ok(());
    }
    println!(
        "{:<24} {:<10} {:<12} Git commit",
        "Release", "Status", "Completed"
    );
    let active = active_release_id(paths)?;
    for release in releases {
        let completed = release
            .completed_at
            .as_deref()
            .unwrap_or("-")
            .chars()
            .take(10)
            .collect::<String>();
        let commit = release.git_commit.chars().take(12).collect::<String>();
        let dirty_marker = if release.worktree_dirty {
            " (dirty)"
        } else {
            ""
        };
        let active_marker = if active.as_deref() == Some(&release.id) {
            " *active"
        } else if release.artifact_dir.is_none() {
            " (no artifacts)"
        } else {
            ""
        };
        let integrity_marker =
            if release.artifact_dir.is_some() && release.artifact_checksums.is_empty() {
                " (unverified artifacts)"
            } else {
                ""
            };
        println!(
            "{:<24} {:<10} {:<12} {commit}{dirty_marker}{active_marker}{integrity_marker}",
            release.id, release.status, completed,
        );
    }
    Ok(())
}

fn validate_rollback_target(
    paths: &Paths,
    profile: &ResolvedProfile,
    release_id: &str,
) -> Result<ReleaseRecord> {
    let release = read_release(paths, release_id)?;
    if release.profile != profile.profile_name {
        bail!(
            "release '{}' belongs to profile '{}', not '{}'",
            release.id,
            release.profile,
            profile.profile_name
        );
    }
    if release.artifact_dir.is_none() || !release_artifact_dir(paths, release_id).is_dir() {
        bail!("release '{release_id}' has no rollback artifacts");
    }
    verify_release_artifacts(paths, &release)?;
    Ok(release)
}

fn print_rollback_plan(paths: &Paths, profile: &ResolvedProfile, release_id: &str) -> Result<()> {
    let _ = deployment_config(profile)?;
    let _ = validate_rollback_target(paths, profile, release_id)?;
    let active = active_release_id(paths)?;
    println!("Rollback plan for profile {}", profile.profile_name);
    println!("Current release: {}", active.as_deref().unwrap_or("none"));
    println!("Target release:  {release_id}");
    println!("1. Ensure the managed database is ready (no migration or restore)");
    println!("2. Stop frontend, backend, and embedding services");
    println!("3. Atomically activate release {release_id}");
    println!("4. Start the archived release services");
    println!("5. Run profile smoke checks");
    println!("6. Reactivate the previous release automatically if steps 4 or 5 fail");
    println!("\nDry run complete; no services or data were changed.");
    Ok(())
}

fn recover_active_release(
    registry: &ConfigRegistry,
    paths: &Paths,
    profile: &ResolvedProfile,
    release_id: &str,
) -> Result<()> {
    let _ = stop_application_services(paths, profile);
    activate_release(paths, release_id)?;
    start_release_services(registry, paths, profile, release_id)?;
    if !run_checks(paths, profile) {
        bail!("recovery smoke checks failed");
    }
    Ok(())
}

fn rollback_profile(
    registry: &ConfigRegistry,
    paths: &Paths,
    profile: &ResolvedProfile,
    release_id: &str,
) -> Result<()> {
    let _lock = acquire_deployment_lock(paths)?;
    let mut target = validate_rollback_target(paths, profile, release_id)?;
    let previous = active_release_id(paths)?
        .ok_or_else(|| anyhow!("no active release is available for safe rollback recovery"))?;
    if previous == release_id {
        bail!("release '{release_id}' is already active");
    }
    let _ = validate_rollback_target(paths, profile, &previous)?;

    ensure_db_running(registry, paths, profile)?;
    ensure_profile_database(paths, profile)?;
    println!(
        "Rolling back profile {} from {} to {} (database unchanged)",
        profile.profile_name, previous, release_id
    );
    stop_application_services(paths, profile)?;
    activate_release(paths, release_id)?;

    let rollback_result = (|| {
        start_release_services(registry, paths, profile, release_id)?;
        if !run_checks(paths, profile) {
            bail!("rollback smoke checks failed");
        }
        Ok(())
    })();

    match rollback_result {
        Ok(()) => {
            target.activated_at = Some(Utc::now().to_rfc3339());
            target.phases.push(ReleasePhase {
                name: format!("rollback activation from {previous}"),
                status: "passed".to_owned(),
                completed_at: Utc::now().to_rfc3339(),
                detail: Some("database unchanged".to_owned()),
            });
            write_release(paths, &target)?;
            println!("\nRollback to {release_id} completed successfully.");
            Ok(())
        }
        Err(rollback_error) => {
            eprintln!(
                "\nRollback target failed; attempting recovery with previous release {previous}..."
            );
            let recovery = recover_active_release(registry, paths, profile, &previous);
            target.phases.push(ReleasePhase {
                name: format!("rollback activation from {previous}"),
                status: "failed".to_owned(),
                completed_at: Utc::now().to_rfc3339(),
                detail: Some(format!("{rollback_error:#}")),
            });
            write_release(paths, &target)?;
            match recovery {
                Ok(()) => Err(rollback_error).context(format!(
                    "rollback to {release_id} failed; previous release {previous} was restored"
                )),
                Err(recovery_error) => bail!(
                    "rollback to {release_id} failed: {rollback_error:#}; recovery of {previous} also failed: {recovery_error:#}"
                ),
            }
        }
    }
}

fn run() -> Result<ExitCode> {
    let repo_root = repo_root()?;
    let registry = load_registry(&repo_root)?;

    let cli = Cli::parse();

    if matches!(cli.command, CommandKind::ListProfiles) {
        list_profiles(&registry);
        return Ok(ExitCode::SUCCESS);
    }

    let profile_name = cli
        .profile
        .ok_or_else(|| anyhow!("profile is required for this command"))?;

    let profile = resolve_profile(&registry, &profile_name)?;
    let paths = state_paths(&profile.state_namespace)?;
    ensure_state_dirs(&paths)?;

    match cli.command {
        CommandKind::ListProfiles => {
            list_profiles(&registry);
            return Ok(ExitCode::SUCCESS);
        }
        CommandKind::Status => {
            let status = show_status(&paths, &profile);
            return Ok(if status == OverallStatus::Healthy {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            });
        }
        CommandKind::Check => {
            return Ok(if run_checks(&paths, &profile) {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            });
        }
        CommandKind::Deploy {
            dry_run,
            allow_dirty,
        } => {
            if dry_run {
                print_deployment_plan(&paths, &profile, allow_dirty)?;
            } else {
                deploy_profile(&registry, &paths, &profile, allow_dirty)?;
            }
        }
        CommandKind::Releases => list_releases(&paths)?,
        CommandKind::Rollback { release, dry_run } => {
            if dry_run {
                print_rollback_plan(&paths, &profile, &release)?;
            } else {
                rollback_profile(&registry, &paths, &profile, &release)?;
            }
        }
        CommandKind::ListUsers => list_users(&paths, &profile)?,
        CommandKind::ExportContributions { username, file } => {
            export_contributions(&registry, &paths, &profile, &username, file.as_deref())?
        }
        CommandKind::ImportContributions {
            username,
            file,
            publish,
        } => import_contributions(&registry, &paths, &profile, &username, &file, publish)?,
        CommandKind::Start { service, source } => {
            print_profile_endpoints(&profile);
            for service in dependency_order(&profile, &service)? {
                start_configured_service(&registry, &paths, &profile, service, source)?;
                if service == ServiceName::Db {
                    ensure_profile_database(&paths, &profile)?;
                }
            }
            println!("\nAll requested services are ready for profile {profile_name}.");
        }
        CommandKind::Stop { service } => {
            let mut order = dependency_order(&profile, &service)?;
            order.reverse();
            for service in order {
                stop_service(&paths, &profile, service)?;
            }
        }
        CommandKind::Restart { service, source } => {
            print_profile_endpoints(&profile);
            let order = dependency_order(&profile, &service)?;
            for service in order.iter().rev().copied() {
                stop_service(&paths, &profile, service)?;
            }
            for service in order {
                start_configured_service(&registry, &paths, &profile, service, source)?;
                if service == ServiceName::Db {
                    ensure_profile_database(&paths, &profile)?;
                }
            }
            println!("\nAll requested services are ready for profile {profile_name}.");
        }
        CommandKind::Log { service, follow } => show_logs(&paths, &service, follow)?,
        CommandKind::Backup { name } => {
            let _ = create_backup(&registry, &paths, &profile, name.as_deref())?;
        }
        CommandKind::VerifyBackup { backup } => {
            ensure_db_running(&registry, &paths, &profile)?;
            verify_backup_file(&paths, &profile, &resolve_backup_path(&paths, &backup))?
        }
        CommandKind::Restore {
            backup,
            allow_profile_mismatch,
        } => restore_backup(&registry, &paths, &profile, &backup, allow_profile_mismatch)?,
        CommandKind::SetPassword {
            username,
            new_password,
        } => set_password(&paths, &profile, &username, &new_password)?,
        CommandKind::SetRole { username, role } => set_role(&paths, &profile, &username, &role)?,
    }

    Ok(ExitCode::SUCCESS)
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("\n=== rimbunctl failed ===");
            eprintln!("{error:#}");
            ExitCode::from(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    fn temporary_paths(label: &str) -> Paths {
        let state_dir = std::env::temp_dir().join(format!(
            "rimbunctl-{label}-{}-{}",
            std::process::id(),
            Utc::now().timestamp_micros()
        ));
        let paths = Paths {
            repo_root: state_dir.join("repo"),
            backup_dir: state_dir.join("backups"),
            log_dir: state_dir.join("logs"),
            pid_dir: state_dir.join("pids"),
            release_dir: state_dir.join("releases"),
            state_dir,
        };
        fs::create_dir_all(&paths.repo_root).expect("create test repository");
        ensure_state_dirs(&paths).expect("create test state directories");
        paths
    }

    fn test_release(id: &str) -> ReleaseRecord {
        ReleaseRecord {
            format_version: 2,
            id: id.to_owned(),
            profile: "dev".to_owned(),
            git_commit: "0123456789abcdef".to_owned(),
            worktree_dirty: false,
            started_at: Utc::now().to_rfc3339(),
            completed_at: Some(Utc::now().to_rfc3339()),
            status: "deployed".to_owned(),
            backup: None,
            artifact_dir: Some(id.to_owned()),
            activated_at: None,
            artifact_checksums: BTreeMap::new(),
            phases: Vec::new(),
        }
    }

    fn serve_status(status: &str) -> (u16, thread::JoinHandle<()>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind test listener");
        let port = listener.local_addr().expect("read test address").port();
        let response = format!("HTTP/1.1 {status}\r\nContent-Length: 0\r\n\r\n");
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept readiness probe");
            let mut request = [0_u8; 512];
            let _ = stream.read(&mut request);
            stream
                .write_all(response.as_bytes())
                .expect("write readiness response");
        });
        (port, handle)
    }

    #[test]
    fn http_service_ready_accepts_success_status() {
        let (port, server) = serve_status("200 OK");

        assert!(http_service_ready(port, "/health"));
        server.join().expect("join test server");
    }

    #[test]
    fn http_service_ready_rejects_error_status() {
        let (port, server) = serve_status("503 Service Unavailable");

        assert!(!http_service_ready(port, "/health"));
        server.join().expect("join test server");
    }

    fn status_report(process: &'static str, readiness: &'static str) -> ServiceStatusReport {
        ServiceStatusReport {
            service: ServiceName::Backend,
            process,
            pid: None,
            readiness,
            endpoint: String::new(),
            log_path: PathBuf::new(),
        }
    }

    #[test]
    fn status_is_healthy_only_when_every_service_and_migrations_are_ready() {
        let reports = vec![
            status_report("running", "ready"),
            status_report("running", "ready"),
        ];

        assert_eq!(classify_status(&reports, true), OverallStatus::Healthy);
        assert_eq!(classify_status(&reports, false), OverallStatus::Degraded);
    }

    #[test]
    fn status_distinguishes_stopped_from_degraded() {
        let stopped = vec![status_report("stopped", "not-ready")];
        let orphaned = vec![status_report("stopped", "ready")];

        assert_eq!(classify_status(&[], true), OverallStatus::Stopped);
        assert_eq!(classify_status(&stopped, false), OverallStatus::Stopped);
        assert_eq!(classify_status(&orphaned, true), OverallStatus::Degraded);
    }

    #[test]
    fn status_command_accepts_a_profile() {
        let cli =
            Cli::try_parse_from(["rimbunctl", "dev", "status"]).expect("parse status command");

        assert_eq!(cli.profile.as_deref(), Some("dev"));
        assert!(matches!(cli.command, CommandKind::Status));
    }

    #[test]
    fn check_command_accepts_a_profile() {
        let cli = Cli::try_parse_from(["rimbunctl", "dev", "check"]).expect("parse check command");

        assert_eq!(cli.profile.as_deref(), Some("dev"));
        assert!(matches!(cli.command, CommandKind::Check));
    }

    #[test]
    fn deploy_command_accepts_safety_flags() {
        let cli = Cli::try_parse_from(["rimbunctl", "dev", "deploy", "--dry-run", "--allow-dirty"])
            .expect("parse deploy command");

        assert!(matches!(
            cli.command,
            CommandKind::Deploy {
                dry_run: true,
                allow_dirty: true
            }
        ));
    }

    #[test]
    fn builtin_dev_profile_has_deployment_configuration() {
        let profile = resolve_profile(&builtin_registry(), "dev").expect("resolve dev profile");
        let deployment = deployment_config(&profile).expect("resolve deployment configuration");

        assert_eq!(deployment.build.len(), 2);
        assert_eq!(deployment.migrate, "{release_dir}/bin/rimbun-migrate");
        assert_eq!(deployment.retention, 5);
        assert_eq!(deployment.artifacts.len(), 5);
        assert_eq!(deployment.run.len(), 3);
    }

    #[test]
    fn deployment_lock_prevents_concurrent_deployments() {
        let paths = temporary_paths("deploy-lock");

        let first = acquire_deployment_lock(&paths).expect("acquire first deployment lock");
        assert!(acquire_deployment_lock(&paths).is_err());
        drop(first);
        let second = acquire_deployment_lock(&paths).expect("reacquire deployment lock");
        drop(second);

        fs::remove_dir_all(&paths.state_dir).expect("remove deployment state directory");
    }

    #[test]
    fn rollback_command_accepts_release_and_dry_run() {
        let cli = Cli::try_parse_from([
            "rimbunctl",
            "dev",
            "rollback",
            "20260821-120000-001",
            "--dry-run",
        ])
        .expect("parse rollback command");

        assert!(matches!(
            cli.command,
            CommandKind::Rollback {
                release,
                dry_run: true
            } if release == "20260821-120000-001"
        ));
    }

    #[test]
    fn release_activation_atomically_switches_current_symlink() {
        let paths = temporary_paths("activate-release");
        let first = "20260821-120000-001";
        let second = "20260821-120001-002";
        fs::create_dir_all(release_artifact_dir(&paths, first)).expect("create first release");
        fs::create_dir_all(release_artifact_dir(&paths, second)).expect("create second release");

        activate_release(&paths, first).expect("activate first release");
        assert_eq!(
            active_release_id(&paths)
                .expect("read active release")
                .as_deref(),
            Some(first)
        );
        activate_release(&paths, second).expect("activate second release");
        assert_eq!(
            active_release_id(&paths)
                .expect("read active release")
                .as_deref(),
            Some(second)
        );

        fs::remove_dir_all(&paths.state_dir).expect("remove activation test directory");
    }

    #[test]
    fn archive_release_copies_binaries_and_frontend() {
        let paths = temporary_paths("archive-release");
        for (path, contents) in [
            ("target/debug/rimbun-api", b"api".as_slice()),
            (
                "target/debug/rimbun-embedding-service",
                b"embedding".as_slice(),
            ),
            ("target/debug/rimbun-migrate", b"migrate".as_slice()),
            ("target/debug/rimbun-static-server", b"static".as_slice()),
            ("web/dist/index.html", b"html".as_slice()),
        ] {
            let path = paths.repo_root.join(path);
            fs::create_dir_all(path.parent().expect("test artifact parent"))
                .expect("create test artifact parent");
            fs::write(path, contents).expect("write test artifact");
        }
        let deployment = ResolvedDeploymentConfig {
            artifacts: BTreeMap::from([
                ("backend".to_owned(), "target/debug/rimbun-api".to_owned()),
                (
                    "embedding".to_owned(),
                    "target/debug/rimbun-embedding-service".to_owned(),
                ),
                (
                    "migrate".to_owned(),
                    "target/debug/rimbun-migrate".to_owned(),
                ),
                (
                    "static".to_owned(),
                    "target/debug/rimbun-static-server".to_owned(),
                ),
                ("frontend".to_owned(), "web/dist".to_owned()),
            ]),
            ..ResolvedDeploymentConfig::default()
        };
        let release_id = "20260821-120000-001";

        let archived = archive_release_artifacts(&paths, &deployment, release_id)
            .expect("archive release artifacts");

        assert_eq!(
            fs::read(archived.join("bin/rimbun-api")).expect("read archived API"),
            b"api"
        );
        assert_eq!(
            fs::read(archived.join("web/index.html")).expect("read archived frontend"),
            b"html"
        );
        let mut release = test_release(release_id);
        release.artifact_checksums = artifact_checksums(&archived).expect("hash artifacts");
        verify_release_artifacts(&paths, &release).expect("verify archived artifacts");
        fs::write(archived.join("web/index.html"), b"changed").expect("alter archived frontend");
        assert!(verify_release_artifacts(&paths, &release).is_err());
        assert!(
            !paths
                .release_dir
                .join(format!(".{release_id}.staging"))
                .exists()
        );
        fs::remove_dir_all(&paths.state_dir).expect("remove archive test directory");
    }

    #[test]
    fn retention_keeps_newest_releases_and_older_active_release() {
        let paths = temporary_paths("release-retention");
        let releases = [
            "20260821-120000-001",
            "20260821-120001-002",
            "20260821-120002-003",
            "20260821-120003-004",
        ];
        for release_id in releases {
            fs::create_dir_all(release_artifact_dir(&paths, release_id))
                .expect("create release artifacts");
            write_release(&paths, &test_release(release_id)).expect("write release record");
        }
        activate_release(&paths, releases[0]).expect("activate oldest release");

        prune_releases(&paths, 2).expect("prune old releases");

        assert!(release_artifact_dir(&paths, releases[0]).exists());
        assert!(!release_artifact_dir(&paths, releases[1]).exists());
        assert!(release_artifact_dir(&paths, releases[2]).exists());
        assert!(release_artifact_dir(&paths, releases[3]).exists());
        fs::remove_dir_all(&paths.state_dir).expect("remove retention test directory");
    }

    #[test]
    fn skipped_checks_do_not_fail_the_run() {
        let results = vec![
            CheckResult {
                name: "required".to_owned(),
                outcome: CheckOutcome::Pass,
                detail: String::new(),
            },
            CheckResult {
                name: "optional".to_owned(),
                outcome: CheckOutcome::Skip,
                detail: String::new(),
            },
        ];

        assert!(checks_succeeded(&results));
    }

    #[test]
    fn any_failed_check_fails_the_run() {
        let results = vec![CheckResult {
            name: "required".to_owned(),
            outcome: CheckOutcome::Fail,
            detail: String::new(),
        }];

        assert!(!checks_succeeded(&results));
    }

    #[test]
    fn verify_backup_command_accepts_a_profile_and_file() {
        let cli = Cli::try_parse_from(["rimbunctl", "dev", "verify-backup", "backup.sql"])
            .expect("parse verify-backup command");

        assert!(matches!(
            cli.command,
            CommandKind::VerifyBackup { backup } if backup == "backup.sql"
        ));
    }

    #[test]
    fn sha256_file_hashes_backup_contents() {
        let path = std::env::temp_dir().join(format!(
            "rimbunctl-sha256-{}-{}.sql",
            std::process::id(),
            Utc::now().timestamp_micros()
        ));
        fs::write(&path, b"abc").expect("write test backup");

        let checksum = sha256_file(&path).expect("hash test backup");

        assert_eq!(
            checksum,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        fs::remove_file(path).expect("remove test backup");
    }

    #[test]
    fn backup_metadata_detects_changes_after_verification() {
        let directory = std::env::temp_dir().join(format!(
            "rimbunctl-metadata-{}-{}",
            std::process::id(),
            Utc::now().timestamp_micros()
        ));
        fs::create_dir_all(&directory).expect("create test backup directory");
        let backup_path = directory.join("backup.sql");
        fs::write(&backup_path, b"valid backup").expect("write test backup");
        let metadata = BackupMetadata {
            format_version: 1,
            profile: "dev".to_owned(),
            database: "rimbun_dev".to_owned(),
            created_at: Utc::now().to_rfc3339(),
            size_bytes: 12,
            sha256: sha256_file(&backup_path).expect("hash test backup"),
            verification: BackupVerification {
                status: "verified".to_owned(),
                verified_at: Some(Utc::now().to_rfc3339()),
            },
        };
        write_backup_metadata(&backup_path, &metadata).expect("write test metadata");

        assert!(backup_verification_label(&backup_path).starts_with("verified"));
        fs::write(&backup_path, b"modified backup").expect("modify test backup");
        assert_eq!(
            backup_verification_label(&backup_path),
            "CORRUPT (checksum mismatch)"
        );

        fs::remove_dir_all(directory).expect("remove test backup directory");
    }
}
