use std::process::ExitCode;

use argon2::{
    Argon2,
    password_hash::{PasswordHasher, SaltString},
};
use rand_core::OsRng;
use sqlx::postgres::PgPoolOptions;

use rimbun_api::{config::Config, db::users};

fn usage() {
    eprintln!(
        "Usage: cargo run -p rimbun-api --bin rimbun-set-password -- <username> <new-password>"
    );
}

#[tokio::main]
async fn main() -> ExitCode {
    let _ = dotenvy::dotenv();

    let mut args = std::env::args().skip(1);
    let Some(username) = args.next() else {
        usage();
        return ExitCode::from(2);
    };
    let Some(new_password) = args.next() else {
        usage();
        return ExitCode::from(2);
    };
    if args.next().is_some() {
        usage();
        return ExitCode::from(2);
    }

    if new_password.len() < 8 {
        eprintln!("Error: new password must be at least 8 characters long");
        return ExitCode::from(2);
    }

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

    let salt = SaltString::generate(&mut OsRng);
    let password_hash = match Argon2::default().hash_password(new_password.as_bytes(), &salt) {
        Ok(hash) => hash.to_string(),
        Err(error) => {
            eprintln!("Error: failed to hash password: {error}");
            return ExitCode::from(1);
        }
    };

    match users::update_password_hash(&pool, user.id, &password_hash).await {
        Ok(Some(_)) => {
            println!("Password updated for @{}", user.username);
            ExitCode::SUCCESS
        }
        Ok(None) => {
            eprintln!("Error: user '{}' disappeared during update", user.username);
            ExitCode::from(1)
        }
        Err(error) => {
            eprintln!("Error: failed to update password: {error}");
            ExitCode::from(1)
        }
    }
}
