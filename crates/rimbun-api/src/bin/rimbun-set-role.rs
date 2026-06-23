use std::process::ExitCode;

use sqlx::postgres::PgPoolOptions;

use rimbun_api::{config::Config, db::users};

fn usage() {
    eprintln!("Usage: cargo run -p rimbun-api --bin rimbun-set-role -- <username> <role>");
    eprintln!("Allowed roles: normal, privileged, admin");
}

fn normalize_role(role: &str) -> Option<&'static str> {
    match role {
        "normal" => Some("normal"),
        "privileged" => Some("privileged"),
        "admin" => Some("admin"),
        _ => None,
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    let _ = dotenvy::dotenv();

    let mut args = std::env::args().skip(1);
    let Some(username) = args.next() else {
        usage();
        return ExitCode::from(2);
    };
    let Some(role_arg) = args.next() else {
        usage();
        return ExitCode::from(2);
    };
    if args.next().is_some() {
        usage();
        return ExitCode::from(2);
    }

    let Some(role) = normalize_role(&role_arg) else {
        eprintln!("Error: invalid role '{role_arg}'");
        usage();
        return ExitCode::from(2);
    };

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

    match users::update_role(&pool, user.id, role).await {
        Ok(Some(updated)) => {
            println!("Role updated for @{} -> {}", updated.username, updated.role);
            ExitCode::SUCCESS
        }
        Ok(None) => {
            eprintln!("Error: user '{}' disappeared during update", user.username);
            ExitCode::from(1)
        }
        Err(error) => {
            eprintln!("Error: failed to update role: {error}");
            ExitCode::from(1)
        }
    }
}
