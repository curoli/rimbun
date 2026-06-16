use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use anyhow::{Context, Result, anyhow, bail};
use chrono::Utc;
use clap::{Parser, Subcommand, ValueEnum};
use nix::{
    sys::{
        signal::{Signal, kill},
        stat::Mode,
    },
    unistd::{Pid, mkfifo},
};
use serde::Deserialize;

const SERVICE_ORDER: [ServiceName; 4] = [
    ServiceName::Db,
    ServiceName::Embedding,
    ServiceName::Backend,
    ServiceName::Frontend,
];

#[derive(Debug, Parser)]
#[command(name = "rimbunctl")]
struct Cli {
    profile: String,
    #[command(subcommand)]
    command: CommandKind,
}

#[derive(Debug, Subcommand)]
enum CommandKind {
    Start {
        #[arg(default_value = "all")]
        service: ServiceTarget,
    },
    Stop {
        #[arg(default_value = "all")]
        service: ServiceTarget,
    },
    Restart {
        #[arg(default_value = "all")]
        service: ServiceTarget,
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
    Restore {
        backup: String,
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
}

#[derive(Debug, Clone, Default)]
struct ResolvedProfile {
    profile_name: String,
    state_namespace: String,
    env: BTreeMap<String, String>,
    services: BTreeMap<ServiceName, ResolvedServiceConfig>,
    database: Option<ResolvedDatabaseConfig>,
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
    run_dir: PathBuf,
}

#[derive(Debug)]
struct ServicePids {
    service_pid: i32,
    logger_pid: i32,
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
        run_dir: state_dir.join("run"),
        state_dir,
    })
}

fn ensure_state_dirs(paths: &Paths) -> Result<()> {
    fs::create_dir_all(&paths.backup_dir)?;
    fs::create_dir_all(&paths.log_dir)?;
    fs::create_dir_all(&paths.pid_dir)?;
    fs::create_dir_all(&paths.run_dir)?;
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
    vars.insert("run_dir".to_owned(), paths.run_dir.display().to_string());

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
            })
        })
        .transpose()?;

    Ok(ResolvedProfile {
        profile_name: profile_name.to_owned(),
        state_namespace,
        env,
        services,
        database,
    })
}

fn pid_path(paths: &Paths, service: ServiceName) -> PathBuf {
    paths.pid_dir.join(format!("{}.pid", service.as_str()))
}

fn log_path(paths: &Paths, service: ServiceName) -> PathBuf {
    paths.log_dir.join(format!("{}.log", service.as_str()))
}

fn fifo_path(paths: &Paths, service: ServiceName) -> PathBuf {
    paths.run_dir.join(format!("{}.pipe", service.as_str()))
}

fn pid_running(pid: i32) -> bool {
    kill(Pid::from_raw(pid), None).is_ok()
}

fn read_pids(paths: &Paths, service: ServiceName) -> Result<Option<ServicePids>> {
    let path = pid_path(paths, service);
    if !path.exists() {
        return Ok(None);
    }

    let raw = fs::read_to_string(&path)?;
    let mut service_pid = None;
    let mut logger_pid = None;
    for line in raw.lines() {
        if let Some(value) = line.strip_prefix("service_pid=") {
            service_pid = Some(value.parse::<i32>()?);
        } else if let Some(value) = line.strip_prefix("logger_pid=") {
            logger_pid = Some(value.parse::<i32>()?);
        }
    }

    match (service_pid, logger_pid) {
        (Some(service_pid), Some(logger_pid)) => Ok(Some(ServicePids {
            service_pid,
            logger_pid,
        })),
        _ => Ok(None),
    }
}

fn write_pids(paths: &Paths, service: ServiceName, pids: &ServicePids) -> Result<()> {
    fs::write(
        pid_path(paths, service),
        format!(
            "service_pid={}\nlogger_pid={}\n",
            pids.service_pid, pids.logger_pid
        ),
    )?;
    Ok(())
}

fn service_status(paths: &Paths, service: ServiceName) -> Result<bool> {
    let Some(pids) = read_pids(paths, service)? else {
        return Ok(false);
    };

    if pid_running(pids.service_pid) || pid_running(pids.logger_pid) {
        return Ok(true);
    }

    let _ = fs::remove_file(pid_path(paths, service));
    Ok(false)
}

fn shell_command(command: &str, workdir: &Path, env: &BTreeMap<String, String>) -> Command {
    let mut cmd = Command::new("bash");
    cmd.arg("-lc").arg(command).current_dir(workdir);
    cmd.envs(env);
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

fn start_logged_command(
    paths: &Paths,
    profile: &ResolvedProfile,
    service: ServiceName,
    workdir: &Path,
    command: &str,
) -> Result<()> {
    let log_path = log_path(paths, service);
    let fifo_path = fifo_path(paths, service);

    fs::write(&log_path, "")?;
    let _ = fs::remove_file(&fifo_path);
    mkfifo(&fifo_path, Mode::S_IRUSR | Mode::S_IWUSR)?;

    let logger_shell = format!(
        "tee -a '{}' < '{}'",
        log_path.display(),
        fifo_path.display()
    );
    let logger = shell_command(&logger_shell, &paths.repo_root, &profile.env).spawn()?;

    let fifo_writer = fs::OpenOptions::new().write(true).open(&fifo_path)?;
    let fifo_writer_err = fifo_writer.try_clone()?;

    let service_child = shell_command(command, workdir, &profile.env)
        .stdout(Stdio::from(fifo_writer))
        .stderr(Stdio::from(fifo_writer_err))
        .spawn()?;

    let _ = fs::remove_file(&fifo_path);
    write_pids(
        paths,
        service,
        &ServicePids {
            service_pid: service_child.id() as i32,
            logger_pid: logger.id() as i32,
        },
    )?;
    println!(
        "Started {} for profile {}",
        service.as_str(),
        profile.profile_name
    );
    Ok(())
}

fn stop_logged_command(paths: &Paths, service: ServiceName) -> Result<()> {
    let Some(pids) = read_pids(paths, service)? else {
        return Ok(());
    };

    let _ = kill(Pid::from_raw(pids.service_pid), Signal::SIGTERM);
    let _ = kill(Pid::from_raw(pids.logger_pid), Signal::SIGTERM);
    let _ = fs::remove_file(pid_path(paths, service));
    Ok(())
}

fn service_config<'a>(
    profile: &'a ResolvedProfile,
    service: ServiceName,
) -> Result<&'a ResolvedServiceConfig> {
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

fn start_service(paths: &Paths, profile: &ResolvedProfile, service: ServiceName) -> Result<()> {
    if service_status(paths, service)? {
        println!("{} already running", service.as_str());
        return Ok(());
    }

    let config = service_config(profile, service)?;
    let workdir = paths.repo_root.join(&config.workdir);
    if let Some(bootstrap) = &config.bootstrap {
        run_shell(bootstrap, &workdir, &profile.env)?;
    }
    start_logged_command(paths, profile, service, &workdir, &config.run)
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

fn ensure_db_running(paths: &Paths, profile: &ResolvedProfile) -> Result<()> {
    if service_status(paths, ServiceName::Db)? {
        return Ok(());
    }

    start_service(paths, profile, ServiceName::Db)
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

fn create_backup(paths: &Paths, profile: &ResolvedProfile, name: Option<&str>) -> Result<()> {
    ensure_db_running(paths, profile)?;

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
    Ok(())
}

fn resolve_backup_path(paths: &Paths, backup: &str) -> PathBuf {
    let candidate = PathBuf::from(backup);
    if candidate.is_absolute() {
        candidate
    } else {
        paths.backup_dir.join(candidate)
    }
}

fn restore_backup(paths: &Paths, profile: &ResolvedProfile, backup: &str) -> Result<()> {
    ensure_restore_safe(paths)?;
    ensure_db_running(paths, profile)?;

    let backup_path = resolve_backup_path(paths, backup);
    if !backup_path.exists() {
        bail!("backup file '{}' not found", backup_path.display());
    }

    let database = database_config(profile)?;
    let command = database
        .restore
        .replace("{file}", &shell_quote(&backup_path.display().to_string()));
    run_shell(&command, &paths.repo_root, &profile.env)?;
    println!("Restored backup {}", backup_path.display());
    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let repo_root = repo_root()?;
    let registry = load_registry(&repo_root)?;
    let profile = resolve_profile(&registry, &cli.profile)?;
    let paths = state_paths(&profile.state_namespace)?;
    ensure_state_dirs(&paths)?;

    match cli.command {
        CommandKind::Start { service } => {
            for service in dependency_order(&profile, &service)? {
                start_service(&paths, &profile, service)?;
            }
        }
        CommandKind::Stop { service } => {
            let mut order = dependency_order(&profile, &service)?;
            order.reverse();
            for service in order {
                stop_service(&paths, &profile, service)?;
            }
        }
        CommandKind::Restart { service } => {
            let order = dependency_order(&profile, &service)?;
            for service in order.iter().rev().copied() {
                stop_service(&paths, &profile, service)?;
            }
            for service in order {
                start_service(&paths, &profile, service)?;
            }
        }
        CommandKind::Log { service, follow } => show_logs(&paths, &service, follow)?,
        CommandKind::Backup { name } => create_backup(&paths, &profile, name.as_deref())?,
        CommandKind::Restore { backup } => restore_backup(&paths, &profile, &backup)?,
        CommandKind::SetPassword {
            username,
            new_password,
        } => set_password(&paths, &profile, &username, &new_password)?,
    }

    Ok(())
}
