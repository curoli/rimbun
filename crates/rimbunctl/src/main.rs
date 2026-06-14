use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use anyhow::{anyhow, bail, Context, Result};
use chrono::Utc;
use clap::{Parser, Subcommand, ValueEnum};
use nix::{
    sys::{
        signal::{kill, Signal},
        stat::Mode,
    },
    unistd::{mkfifo, Pid},
};
use serde::Deserialize;

const SERVICE_ORDER: [ServiceName; 4] = [
    ServiceName::Db,
    ServiceName::Embedding,
    ServiceName::Backend,
    ServiceName::Frontend,
];
const STOP_ORDER: [ServiceName; 4] = [
    ServiceName::Frontend,
    ServiceName::Backend,
    ServiceName::Embedding,
    ServiceName::Db,
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
        match value {
            "db" => Ok(Self::One(ServiceName::Db)),
            "embedding" => Ok(Self::One(ServiceName::Embedding)),
            "backend" => Ok(Self::One(ServiceName::Backend)),
            "frontend" => Ok(Self::One(ServiceName::Frontend)),
            _ => Err(format!("unknown service '{value}'")),
        }
    }
}

#[derive(Debug, Deserialize)]
struct FileConfig {
    profiles: BTreeMap<String, ProfileConfig>,
}

#[derive(Debug, Clone, Deserialize)]
struct ProfileConfig {
    services: BTreeMap<String, ServiceConfig>,
    database: Option<DatabaseConfig>,
}

#[derive(Debug, Clone, Deserialize)]
struct ServiceConfig {
    workdir: String,
    run: String,
    bootstrap: Option<String>,
    stop: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct DatabaseConfig {
    backup: String,
    restore: String,
}

#[derive(Debug)]
struct Paths {
    repo_root: PathBuf,
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

fn state_paths(profile: &str) -> Result<Paths> {
    let repo_root = repo_root()?;
    let state_dir = repo_root.join(".rimbun");
    let profile_dir = state_dir.join(profile);
    Ok(Paths {
        repo_root,
        backup_dir: profile_dir.join("backups"),
        log_dir: profile_dir.join("logs"),
        pid_dir: profile_dir.join("pids"),
        run_dir: profile_dir.join("run"),
    })
}

fn ensure_state_dirs(paths: &Paths) -> Result<()> {
    fs::create_dir_all(&paths.backup_dir)?;
    fs::create_dir_all(&paths.log_dir)?;
    fs::create_dir_all(&paths.pid_dir)?;
    fs::create_dir_all(&paths.run_dir)?;
    Ok(())
}

fn load_profiles(repo_root: &Path) -> Result<BTreeMap<String, ProfileConfig>> {
    let mut profiles = BTreeMap::new();
    profiles.insert("dev".to_owned(), default_dev_profile());

    let config_path = repo_root.join("rimbunctl.toml");
    if config_path.exists() {
        let raw = fs::read_to_string(&config_path)
            .with_context(|| format!("failed to read {}", config_path.display()))?;
        let file_config: FileConfig = toml::from_str(&raw)
            .with_context(|| format!("failed to parse {}", config_path.display()))?;
        profiles.extend(file_config.profiles);
    }

    Ok(profiles)
}

fn default_dev_profile() -> ProfileConfig {
    let mut services = BTreeMap::new();
    services.insert(
        "db".to_owned(),
        ServiceConfig {
            workdir: ".".to_owned(),
            bootstrap: Some("docker compose up -d postgres".to_owned()),
            run: "docker compose logs -f postgres".to_owned(),
            stop: Some("docker compose stop postgres >/dev/null".to_owned()),
        },
    );
    services.insert(
        "embedding".to_owned(),
        ServiceConfig {
            workdir: ".".to_owned(),
            bootstrap: None,
            run: "cargo run -p rimbun-embedding-service --bin rimbun-embedding-service".to_owned(),
            stop: None,
        },
    );
    services.insert(
        "backend".to_owned(),
        ServiceConfig {
            workdir: ".".to_owned(),
            bootstrap: None,
            run: "cargo run -p rimbun-api --bin rimbun-api".to_owned(),
            stop: None,
        },
    );
    services.insert(
        "frontend".to_owned(),
        ServiceConfig {
            workdir: "web".to_owned(),
            bootstrap: Some("test -d node_modules || npm install".to_owned()),
            run: "npm run dev -- --host 127.0.0.1 --port 5173 < /dev/null".to_owned(),
            stop: None,
        },
    );
    ProfileConfig {
        services,
        database: Some(DatabaseConfig {
            backup: "docker compose exec -T postgres pg_dump -U postgres -d rimbun > {file}"
                .to_owned(),
            restore: "docker compose exec -T postgres psql -U postgres -d rimbun < {file}"
                .to_owned(),
        }),
    }
}

fn profile_service<'a>(profile: &'a ProfileConfig, service: ServiceName) -> Result<&'a ServiceConfig> {
    profile
        .services
        .get(service.as_str())
        .ok_or_else(|| anyhow!("service '{}' missing from profile", service.as_str()))
}

fn profile_database(profile: &ProfileConfig) -> Result<&DatabaseConfig> {
    profile
        .database
        .as_ref()
        .ok_or_else(|| anyhow!("profile has no database backup configuration"))
}

fn service_list(target: &ServiceTarget) -> Vec<ServiceName> {
    match target {
        ServiceTarget::All => SERVICE_ORDER.to_vec(),
        ServiceTarget::One(service) => vec![*service],
    }
}

fn stop_list(target: &ServiceTarget) -> Vec<ServiceName> {
    match target {
        ServiceTarget::All => STOP_ORDER.to_vec(),
        ServiceTarget::One(service) => vec![*service],
    }
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

fn shell_command(command: &str, workdir: &Path) -> Command {
    let mut cmd = Command::new("bash");
    cmd.arg("-lc").arg(command).current_dir(workdir);
    cmd
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn run_shell(command: &str, workdir: &Path) -> Result<()> {
    let status = shell_command(command, workdir).status()?;
    if !status.success() {
        bail!("command failed: {command}");
    }
    Ok(())
}

fn start_logged_command(
    paths: &Paths,
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
    let logger = shell_command(&logger_shell, &paths.repo_root).spawn()?;

    let fifo_writer = fs::OpenOptions::new().write(true).open(&fifo_path)?;
    let fifo_writer_err = fifo_writer.try_clone()?;

    let service_child = shell_command(command, workdir)
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
    println!("Started {}", service.as_str());
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

fn start_service(paths: &Paths, profile: &ProfileConfig, service: ServiceName) -> Result<()> {
    if service_status(paths, service)? {
        println!("{} already running", service.as_str());
        return Ok(());
    }

    let config = profile_service(profile, service)?;
    let workdir = paths.repo_root.join(&config.workdir);
    if let Some(bootstrap) = &config.bootstrap {
        run_shell(bootstrap, &workdir)?;
    }
    start_logged_command(paths, service, &workdir, &config.run)
}

fn stop_service(paths: &Paths, profile: &ProfileConfig, service: ServiceName) -> Result<()> {
    stop_logged_command(paths, service)?;
    if let Some(command) = &profile_service(profile, service)?.stop {
        let workdir = paths.repo_root.join(&profile_service(profile, service)?.workdir);
        run_shell(command, &workdir)?;
    }
    println!("Stopped {}", service.as_str());
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

fn set_password(paths: &Paths, username: &str, new_password: &str) -> Result<()> {
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
        .status()?;

    if !status.success() {
        bail!("failed to set password");
    }
    Ok(())
}

fn ensure_db_running(paths: &Paths, profile: &ProfileConfig) -> Result<()> {
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

fn create_backup(paths: &Paths, profile: &ProfileConfig, name: Option<&str>) -> Result<()> {
    ensure_db_running(paths, profile)?;

    let database = profile_database(profile)?;
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

    run_shell(&command, &paths.repo_root)?;
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

fn restore_backup(paths: &Paths, profile: &ProfileConfig, backup: &str) -> Result<()> {
    ensure_restore_safe(paths)?;
    ensure_db_running(paths, profile)?;

    let backup_path = resolve_backup_path(paths, backup);
    if !backup_path.exists() {
        bail!("backup file '{}' not found", backup_path.display());
    }

    let database = profile_database(profile)?;
    let command = database
        .restore
        .replace("{file}", &shell_quote(&backup_path.display().to_string()));
    run_shell(&command, &paths.repo_root)?;
    println!("Restored backup {}", backup_path.display());
    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let paths = state_paths(&cli.profile)?;
    ensure_state_dirs(&paths)?;

    let profiles = load_profiles(&paths.repo_root)?;
    let profile = profiles
        .get(&cli.profile)
        .ok_or_else(|| anyhow!("unsupported profile '{}'", cli.profile))?;

    match cli.command {
        CommandKind::Start { service } => {
            for service in service_list(&service) {
                start_service(&paths, profile, service)?;
            }
        }
        CommandKind::Stop { service } => {
            for service in stop_list(&service) {
                stop_service(&paths, profile, service)?;
            }
        }
        CommandKind::Restart { service } => {
            for service in stop_list(&service) {
                stop_service(&paths, profile, service)?;
            }
            for service in service_list(&service) {
                start_service(&paths, profile, service)?;
            }
        }
        CommandKind::Log { service, follow } => show_logs(&paths, &service, follow)?,
        CommandKind::Backup { name } => create_backup(&paths, profile, name.as_deref())?,
        CommandKind::Restore { backup } => restore_backup(&paths, profile, &backup)?,
        CommandKind::SetPassword {
            username,
            new_password,
        } => set_password(&paths, &username, &new_password)?,
    }

    Ok(())
}
