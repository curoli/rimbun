use chrono::{DateTime, Duration, Utc};
use sqlx::{FromRow, PgPool};

#[derive(Debug, Clone, FromRow)]
pub struct SessionRecord {
    pub id: uuid::Uuid,
    pub token: String,
    pub user_id: uuid::Uuid,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewSession {
    pub id: uuid::Uuid,
    pub token: String,
    pub user_id: uuid::Uuid,
}

pub fn session_expiry() -> DateTime<Utc> {
    Utc::now() + Duration::days(30)
}

pub async fn create(pool: &PgPool, session: &NewSession) -> anyhow::Result<SessionRecord> {
    let record = sqlx::query_as::<_, SessionRecord>(
        r#"
        insert into user_sessions (id, token, user_id, expires_at)
        values ($1, $2, $3, $4)
        returning id, token, user_id, created_at, expires_at
        "#,
    )
    .bind(session.id)
    .bind(&session.token)
    .bind(session.user_id)
    .bind(session_expiry())
    .fetch_one(pool)
    .await?;

    Ok(record)
}

pub async fn find_valid_by_token(
    pool: &PgPool,
    token: &str,
) -> anyhow::Result<Option<SessionRecord>> {
    let record = sqlx::query_as::<_, SessionRecord>(
        r#"
        select id, token, user_id, created_at, expires_at
        from user_sessions
        where token = $1 and expires_at > now()
        "#,
    )
    .bind(token)
    .fetch_optional(pool)
    .await?;

    Ok(record)
}

pub async fn delete_by_token(pool: &PgPool, token: &str) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        delete from user_sessions
        where token = $1
        "#,
    )
    .bind(token)
    .execute(pool)
    .await?;

    Ok(())
}
