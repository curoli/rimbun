use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool};

#[derive(Debug, Clone, FromRow, serde::Serialize)]
pub struct UserRecord {
    pub id: uuid::Uuid,
    pub username: String,
    pub display_name: String,
    pub email: String,
    pub password_hash: String,
    pub role: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewUser {
    pub id: uuid::Uuid,
    pub username: String,
    pub display_name: String,
    pub email: String,
    pub password_hash: String,
    pub role: String,
}

pub async fn create(pool: &PgPool, user: &NewUser) -> anyhow::Result<UserRecord> {
    let record = sqlx::query_as::<_, UserRecord>(
        r#"
        insert into users (id, username, display_name, email, password_hash, role)
        values ($1, $2, $3, $4, $5, $6)
        returning id, username, display_name, email, password_hash, role, created_at
        "#,
    )
    .bind(user.id)
    .bind(&user.username)
    .bind(&user.display_name)
    .bind(&user.email)
    .bind(&user.password_hash)
    .bind(&user.role)
    .fetch_one(pool)
    .await?;

    Ok(record)
}

pub async fn find_by_id(pool: &PgPool, user_id: uuid::Uuid) -> anyhow::Result<Option<UserRecord>> {
    let record = sqlx::query_as::<_, UserRecord>(
        r#"
        select id, username, display_name, email, password_hash, role, created_at
        from users
        where id = $1
        "#,
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?;

    Ok(record)
}

pub async fn find_by_login_identifier(
    pool: &PgPool,
    identifier: &str,
) -> anyhow::Result<Option<UserRecord>> {
    let record = sqlx::query_as::<_, UserRecord>(
        r#"
        select id, username, display_name, email, password_hash, role, created_at
        from users
        where lower(username) = lower($1) or lower(email) = lower($1)
        "#,
    )
    .bind(identifier)
    .fetch_optional(pool)
    .await?;

    Ok(record)
}
