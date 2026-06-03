use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct SiteSettingsRecord {
    pub brand_name: String,
    pub browser_title: String,
    pub updated_at: DateTime<Utc>,
}

pub fn default_settings() -> SiteSettingsRecord {
    SiteSettingsRecord {
        brand_name: "Rimbun".to_owned(),
        browser_title: "Rimbun".to_owned(),
        updated_at: Utc::now(),
    }
}

pub async fn get(pool: &PgPool) -> anyhow::Result<SiteSettingsRecord> {
    let record = sqlx::query_as::<_, SiteSettingsRecord>(
        r#"
        select brand_name, browser_title, updated_at
        from site_settings
        where id = 1
        "#,
    )
    .fetch_optional(pool)
    .await?;

    Ok(record.unwrap_or_else(default_settings))
}

pub async fn upsert(
    pool: &PgPool,
    brand_name: &str,
    browser_title: &str,
) -> anyhow::Result<SiteSettingsRecord> {
    let record = sqlx::query_as::<_, SiteSettingsRecord>(
        r#"
        insert into site_settings (id, brand_name, browser_title)
        values (1, $1, $2)
        on conflict (id) do update set
          brand_name = excluded.brand_name,
          browser_title = excluded.browser_title,
          updated_at = now()
        returning brand_name, browser_title, updated_at
        "#,
    )
    .bind(brand_name)
    .bind(browser_title)
    .fetch_one(pool)
    .await?;

    Ok(record)
}
