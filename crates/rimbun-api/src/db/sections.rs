use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{FromRow, PgPool, Postgres, Transaction};

fn normalized_order<T: Copy + PartialEq>(
    items: Vec<T>,
    moved_item: Option<T>,
    target_position: Option<i32>,
) -> Vec<T> {
    let mut items = items;

    if let Some(moved_item) = moved_item
        && let Some(index) = items.iter().position(|item| *item == moved_item)
    {
        let moved = items.remove(index);
        let requested = target_position.unwrap_or(items.len() as i32).max(0) as usize;
        let insert_at = requested.min(items.len());
        items.insert(insert_at, moved);
    }

    items
}

#[cfg_attr(not(test), allow(dead_code))]
fn descendant_rewritten_path(path: &str, old_prefix: &str, new_prefix: &str) -> Option<String> {
    path.strip_prefix(&(old_prefix.to_owned() + "/"))
        .map(|suffix| format!("{new_prefix}/{suffix}"))
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct SectionRecord {
    pub id: uuid::Uuid,
    pub document_id: uuid::Uuid,
    pub parent_id: Option<uuid::Uuid>,
    pub title: String,
    pub has_heading: bool,
    pub has_own_text: bool,
    pub position: i32,
    pub path: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewSection {
    pub id: uuid::Uuid,
    pub document_id: uuid::Uuid,
    pub parent_id: Option<uuid::Uuid>,
    pub title: String,
    pub has_heading: bool,
    pub has_own_text: bool,
    pub position: i32,
    pub path: String,
}

pub async fn create(pool: &PgPool, section: &NewSection) -> anyhow::Result<SectionRecord> {
    let record = sqlx::query_as::<_, SectionRecord>(
        r#"
        insert into sections (id, document_id, parent_id, title, has_heading, has_own_text, position, path)
        values ($1, $2, $3, $4, $5, $6, $7, $8)
        returning id, document_id, parent_id, title, has_heading, has_own_text, position, path, created_at
        "#,
    )
    .bind(section.id)
    .bind(section.document_id)
    .bind(section.parent_id)
    .bind(&section.title)
    .bind(section.has_heading)
    .bind(section.has_own_text)
    .bind(section.position)
    .bind(&section.path)
    .fetch_one(pool)
    .await?;

    Ok(record)
}

pub async fn list_by_document(
    pool: &PgPool,
    document_id: uuid::Uuid,
) -> anyhow::Result<Vec<SectionRecord>> {
    let records = sqlx::query_as::<_, SectionRecord>(
        r#"
        select id, document_id, parent_id, title, has_heading, has_own_text, position, path, created_at
        from sections
        where document_id = $1
        order by path asc, position asc, created_at asc
        "#,
    )
    .bind(document_id)
    .fetch_all(pool)
    .await?;

    Ok(records)
}

pub async fn find_by_id(
    pool: &PgPool,
    section_id: uuid::Uuid,
) -> anyhow::Result<Option<SectionRecord>> {
    let record = sqlx::query_as::<_, SectionRecord>(
        r#"
        select id, document_id, parent_id, title, has_heading, has_own_text, position, path, created_at
        from sections
        where id = $1
        "#,
    )
    .bind(section_id)
    .fetch_optional(pool)
    .await?;

    Ok(record)
}

pub async fn update_title_position(
    pool: &PgPool,
    section_id: uuid::Uuid,
    title: &str,
    has_heading: bool,
    has_own_text: bool,
    position: i32,
) -> anyhow::Result<Option<SectionRecord>> {
    let record = sqlx::query_as::<_, SectionRecord>(
        r#"
        update sections
        set title = $2, has_heading = $3, has_own_text = $4, position = $5
        where id = $1
        returning id, document_id, parent_id, title, has_heading, has_own_text, position, path, created_at
        "#,
    )
    .bind(section_id)
    .bind(title)
    .bind(has_heading)
    .bind(has_own_text)
    .bind(position)
    .fetch_optional(pool)
    .await?;

    Ok(record)
}

async fn list_group_sections(
    tx: &mut Transaction<'_, Postgres>,
    document_id: uuid::Uuid,
    parent_id: Option<uuid::Uuid>,
) -> anyhow::Result<Vec<SectionRecord>> {
    let records = sqlx::query_as::<_, SectionRecord>(
        r#"
        select id, document_id, parent_id, title, has_heading, has_own_text, position, path, created_at
        from sections
        where document_id = $1
          and parent_id is not distinct from $2
        order by position asc, created_at asc
        "#,
    )
    .bind(document_id)
    .bind(parent_id)
    .fetch_all(&mut **tx)
    .await?;

    Ok(records)
}

async fn normalize_group_positions(
    tx: &mut Transaction<'_, Postgres>,
    document_id: uuid::Uuid,
    parent_id: Option<uuid::Uuid>,
    moved_section_id: Option<uuid::Uuid>,
    target_position: Option<i32>,
) -> anyhow::Result<()> {
    let group = list_group_sections(tx, document_id, parent_id).await?;
    let ordered_ids = normalized_order(
        group.into_iter().map(|section| section.id).collect(),
        moved_section_id,
        target_position,
    );

    // First move every sibling into a temporary negative range so the unique
    // index on (document_id, parent_id, position) cannot trip during reordering.
    for (index, section_id) in ordered_ids.iter().enumerate() {
        sqlx::query(
            r#"
            update sections
            set position = $2
            where id = $1
            "#,
        )
        .bind(section_id)
        .bind(-((index as i32) + 1))
        .execute(&mut **tx)
        .await?;
    }

    for (index, section_id) in ordered_ids.iter().enumerate() {
        sqlx::query(
            r#"
            update sections
            set position = $2
            where id = $1
            "#,
        )
        .bind(section_id)
        .bind(index as i32)
        .execute(&mut **tx)
        .await?;
    }

    Ok(())
}

pub async fn move_section(
    pool: &PgPool,
    section_id: uuid::Uuid,
    title: &str,
    has_heading: bool,
    has_own_text: bool,
    parent_id: Option<uuid::Uuid>,
    position: i32,
) -> anyhow::Result<Option<SectionRecord>> {
    let mut tx = pool.begin().await?;

    let current = sqlx::query_as::<_, SectionRecord>(
        r#"
        select id, document_id, parent_id, title, has_heading, has_own_text, position, path, created_at
        from sections
        where id = $1
        "#,
    )
    .bind(section_id)
    .fetch_optional(&mut *tx)
    .await?;

    let Some(current) = current else {
        tx.rollback().await?;
        return Ok(None);
    };

    let old_parent_id = current.parent_id;
    let old_path = current.path.clone();
    let new_path = if let Some(parent_id) = parent_id {
        let parent = sqlx::query_as::<_, SectionRecord>(
            r#"
            select id, document_id, parent_id, title, has_heading, has_own_text, position, path, created_at
            from sections
            where id = $1
            "#,
        )
        .bind(parent_id)
        .fetch_one(&mut *tx)
        .await?;

        format!("{}/{}", parent.path, section_id)
    } else {
        section_id.to_string()
    };

    // Park the moved section in a temporary negative slot first so that moving
    // into an occupied sibling position cannot violate the unique index.
    let temporary_position = -1_000_000_000;

    sqlx::query_as::<_, SectionRecord>(
        r#"
        update sections
        set title = $2, has_heading = $3, has_own_text = $4, parent_id = $5, position = $6, path = $7
        where id = $1
        returning id, document_id, parent_id, title, has_heading, has_own_text, position, path, created_at
        "#,
    )
    .bind(section_id)
    .bind(title)
    .bind(has_heading)
    .bind(has_own_text)
    .bind(parent_id)
    .bind(temporary_position)
    .bind(&new_path)
    .fetch_one(&mut *tx)
    .await?;

    if old_path != new_path {
        let suffix_start = old_path.len() as i32 + 1;
        sqlx::query(
            r#"
            update sections
            set path = $2 || substring(path from $3)
            where path like $1 || '/%'
            "#,
        )
        .bind(&old_path)
        .bind(&new_path)
        .bind(suffix_start)
        .execute(&mut *tx)
        .await?;
    }

    if old_parent_id != parent_id {
        normalize_group_positions(&mut tx, current.document_id, old_parent_id, None, None).await?;
        normalize_group_positions(
            &mut tx,
            current.document_id,
            parent_id,
            Some(section_id),
            Some(position),
        )
        .await?;
    } else {
        normalize_group_positions(
            &mut tx,
            current.document_id,
            parent_id,
            Some(section_id),
            Some(position),
        )
        .await?;
    }

    let record = sqlx::query_as::<_, SectionRecord>(
        r#"
        select id, document_id, parent_id, title, has_heading, has_own_text, position, path, created_at
        from sections
        where id = $1
        "#,
    )
    .bind(section_id)
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(Some(record))
}

pub async fn delete_subtree(pool: &PgPool, section_id: uuid::Uuid) -> anyhow::Result<bool> {
    let mut tx = pool.begin().await?;
    let current = sqlx::query_as::<_, SectionRecord>(
        r#"
        select id, document_id, parent_id, title, has_heading, has_own_text, position, path, created_at
        from sections
        where id = $1
        for update
        "#,
    )
    .bind(section_id)
    .fetch_optional(&mut *tx)
    .await?;

    let Some(current) = current else {
        tx.rollback().await?;
        return Ok(false);
    };

    let subtree_submission_ids = r#"
        select submission.id
        from submissions submission
        join sections section on section.id = submission.section_id
        where section.path = $1 or section.path like $1 || '/%'
    "#;

    sqlx::query(&format!(
        "update submissions set base_submission_id = null where base_submission_id in ({subtree_submission_ids})"
    ))
    .bind(&current.path)
    .execute(&mut *tx)
    .await?;
    sqlx::query(&format!(
        "update submissions set superseded_by = null where superseded_by in ({subtree_submission_ids})"
    ))
    .bind(&current.path)
    .execute(&mut *tx)
    .await?;
    sqlx::query(&format!(
        "update drafts set base_submission_id = null where base_submission_id in ({subtree_submission_ids})"
    ))
    .bind(&current.path)
    .execute(&mut *tx)
    .await?;
    sqlx::query(&format!(
        "delete from user_section_preferences where preferred_base_submission_id in ({subtree_submission_ids})"
    ))
    .bind(&current.path)
    .execute(&mut *tx)
    .await?;

    sqlx::query("delete from sections where id = $1")
        .bind(section_id)
        .execute(&mut *tx)
        .await?;

    normalize_group_positions(&mut tx, current.document_id, current.parent_id, None, None).await?;

    tx.commit().await?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::{descendant_rewritten_path, normalized_order};

    #[test]
    fn normalized_order_moves_item_up() {
        let ordered = normalized_order(vec![1, 2, 3, 4], Some(3), Some(1));
        assert_eq!(ordered, vec![1, 3, 2, 4]);
    }

    #[test]
    fn normalized_order_moves_item_down() {
        let ordered = normalized_order(vec![1, 2, 3, 4], Some(2), Some(3));
        assert_eq!(ordered, vec![1, 3, 4, 2]);
    }

    #[test]
    fn normalized_order_clamps_negative_positions() {
        let ordered = normalized_order(vec![1, 2, 3], Some(3), Some(-5));
        assert_eq!(ordered, vec![3, 1, 2]);
    }

    #[test]
    fn normalized_order_clamps_large_positions() {
        let ordered = normalized_order(vec![1, 2, 3], Some(1), Some(99));
        assert_eq!(ordered, vec![2, 3, 1]);
    }

    #[test]
    fn descendant_path_rewrite_updates_direct_child() {
        let rewritten = descendant_rewritten_path("root/a/b", "root/a", "root/x");
        assert_eq!(rewritten.as_deref(), Some("root/x/b"));
    }

    #[test]
    fn descendant_path_rewrite_updates_nested_descendant() {
        let rewritten = descendant_rewritten_path("root/a/b/c", "root/a", "root/x");
        assert_eq!(rewritten.as_deref(), Some("root/x/b/c"));
    }

    #[test]
    fn descendant_path_rewrite_ignores_unrelated_path() {
        let rewritten = descendant_rewritten_path("root/q/b", "root/a", "root/x");
        assert_eq!(rewritten, None);
    }
}
