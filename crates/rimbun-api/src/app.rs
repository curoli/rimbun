use axum::{
    routing::{get, post, put},
    Router,
};
use sqlx::postgres::PgPoolOptions;

use crate::{config::Config, state::AppState};

pub async fn build(config: Config) -> anyhow::Result<Router> {
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&config.database_url)
        .await?;

    sqlx::migrate!("../../migrations").run(&pool).await?;

    let state = AppState::new(pool, config);

    let router = Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/api/me", get(crate::handlers::auth::me).patch(crate::handlers::auth::update_me))
        .route(
            "/api/me/change-password",
            post(crate::handlers::auth::change_password),
        )
        .route("/api/users", get(crate::handlers::users::list))
        .route("/api/auth/register", post(crate::handlers::auth::register))
        .route("/api/auth/login", post(crate::handlers::auth::login))
        .route("/api/auth/logout", post(crate::handlers::auth::logout))
        .route(
            "/api/documents",
            get(crate::handlers::documents::list).post(crate::handlers::documents::create),
        )
        .route("/api/documents/{id}", get(crate::handlers::documents::show))
        .route(
            "/api/documents/{id}/sections",
            post(crate::handlers::sections::create),
        )
        .route(
            "/api/sections/{id}",
            get(crate::handlers::sections::show).patch(crate::handlers::sections::update),
        )
        .route("/api/sections/{id}/view", get(crate::handlers::sections::view))
        .route("/api/sections/{id}/draft", put(crate::handlers::drafts::save))
        .route(
            "/api/sections/{id}/publish",
            post(crate::handlers::submissions::publish),
        )
        .route(
            "/api/sections/{id}/submissions",
            get(crate::handlers::submissions::list),
        )
        .route(
            "/api/sections/{id}/compare",
            get(crate::handlers::compare::section_compare),
        )
        .route(
            "/api/sections/{id}/preferences/base-submission",
            put(crate::handlers::preferences::set_base),
        )
        .route(
            "/api/submissions/{id}/moderate",
            post(crate::handlers::moderation::update),
        )
        .with_state(state);

    Ok(router)
}
