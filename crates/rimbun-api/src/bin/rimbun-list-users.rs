use std::process::ExitCode;

use sqlx::postgres::PgPoolOptions;

use rimbun_api::{config::Config, db::users};

#[tokio::main]
async fn main() -> ExitCode {
    let _ = dotenvy::dotenv();

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

    let users = match users::list_all(&pool).await {
        Ok(users) => users,
        Err(error) => {
            eprintln!("Error: failed to load users: {error}");
            return ExitCode::from(1);
        }
    };

    println!("USERNAME\tDISPLAY NAME\tEMAIL\tROLE");
    for user in users {
        println!(
            "{}\t{}\t{}\t{}",
            user.username, user.display_name, user.email, user.role
        );
    }

    ExitCode::SUCCESS
}
