use axum::{
    body::{Body, to_bytes},
    http::{HeaderMap, Method, Request, StatusCode, header},
};
use rimbun_api::{app, config::Config};
use serde_json::json;
use sqlx::{PgPool, postgres::PgPoolOptions};
use tower::util::ServiceExt;

async fn test_pool() -> Option<PgPool> {
    let database_url = std::env::var("TEST_DATABASE_URL").ok()?;
    PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .ok()
}

async fn reset_schema(pool: &PgPool) {
    sqlx::query("drop schema public cascade")
        .execute(pool)
        .await
        .expect("drop schema");
    sqlx::query("create schema public")
        .execute(pool)
        .await
        .expect("create schema");
    sqlx::migrate!("../../migrations")
        .run(pool)
        .await
        .expect("run migrations");
}

async fn seed_user_with_role(pool: &PgPool, role: &str) -> (uuid::Uuid, String) {
    let user_id = uuid::Uuid::new_v4();
    let session_token = uuid::Uuid::new_v4().to_string();

    sqlx::query(
        r#"
        insert into users (id, username, display_name, email, password_hash, role)
        values ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(user_id)
    .bind(format!("user-{user_id}"))
    .bind("Privileged User")
    .bind(format!("{user_id}@example.test"))
    .bind("not-used")
    .bind(role)
    .execute(pool)
    .await
    .expect("insert user");

    sqlx::query(
        r#"
        insert into user_sessions (id, token, user_id, expires_at)
        values ($1, $2, $3, now() + interval '1 day')
        "#,
    )
    .bind(uuid::Uuid::new_v4())
    .bind(&session_token)
    .bind(user_id)
    .execute(pool)
    .await
    .expect("insert session");

    (user_id, session_token)
}

async fn seed_admin_user(pool: &PgPool) -> (uuid::Uuid, String) {
    seed_user_with_role(pool, "admin").await
}

async fn seed_document_tree(
    pool: &PgPool,
    created_by: uuid::Uuid,
) -> (uuid::Uuid, uuid::Uuid, uuid::Uuid, uuid::Uuid) {
    let document_id = uuid::Uuid::new_v4();
    let parent_a = uuid::Uuid::new_v4();
    let parent_b = uuid::Uuid::new_v4();
    let child = uuid::Uuid::new_v4();

    sqlx::query(
        r#"
        insert into documents (id, slug, title, visibility, markdown_policy, created_by)
        values ($1, $2, $3, 'authenticated', '{}'::jsonb, $4)
        "#,
    )
    .bind(document_id)
    .bind(format!("doc-{document_id}"))
    .bind("Test Document")
    .bind(created_by)
    .execute(pool)
    .await
    .expect("insert document");

    for (section_id, parent_id, title, position, path) in [
        (parent_a, None, "Parent A", 0, parent_a.to_string()),
        (parent_b, None, "Parent B", 1, parent_b.to_string()),
        (
            child,
            Some(parent_a),
            "Child",
            0,
            format!("{parent_a}/{child}"),
        ),
    ] {
        sqlx::query(
            r#"
            insert into sections (id, document_id, parent_id, title, position, path)
            values ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(section_id)
        .bind(document_id)
        .bind(parent_id)
        .bind(title)
        .bind(position)
        .bind(path)
        .execute(pool)
        .await
        .expect("insert section");
    }

    (document_id, parent_a, parent_b, child)
}

async fn seed_nested_descendant(
    pool: &PgPool,
    document_id: uuid::Uuid,
    child: uuid::Uuid,
) -> uuid::Uuid {
    let grandchild = uuid::Uuid::new_v4();
    sqlx::query(
        r#"
        insert into sections (id, document_id, parent_id, title, position, path)
        values ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(grandchild)
    .bind(document_id)
    .bind(child)
    .bind("Grandchild")
    .bind(0)
    .bind(format!("{child}/{grandchild}"))
    .execute(pool)
    .await
    .expect("insert grandchild");
    grandchild
}

async fn seed_single_section_document(
    pool: &PgPool,
    created_by: uuid::Uuid,
) -> (uuid::Uuid, uuid::Uuid) {
    let document_id = uuid::Uuid::new_v4();
    let section_id = uuid::Uuid::new_v4();

    sqlx::query(
        r#"
        insert into documents (id, slug, title, visibility, markdown_policy, created_by)
        values ($1, $2, $3, 'authenticated', '{}'::jsonb, $4)
        "#,
    )
    .bind(document_id)
    .bind(format!("doc-{document_id}"))
    .bind("Publish Document")
    .bind(created_by)
    .execute(pool)
    .await
    .expect("insert document");

    sqlx::query(
        r#"
        insert into sections (id, document_id, parent_id, title, position, path)
        values ($1, $2, null, $3, 0, $4)
        "#,
    )
    .bind(section_id)
    .bind(document_id)
    .bind("Section")
    .bind(section_id.to_string())
    .execute(pool)
    .await
    .expect("insert section");

    (document_id, section_id)
}

async fn seed_variant_collection(pool: &PgPool, created_by: uuid::Uuid) -> uuid::Uuid {
    let collection_id = uuid::Uuid::new_v4();
    sqlx::query(
        r#"
        insert into variant_collections (id, name, description, created_by)
        values ($1, $2, $3, $4)
        "#,
    )
    .bind(collection_id)
    .bind("Cities")
    .bind("Variant collection for test runs")
    .bind(created_by)
    .execute(pool)
    .await
    .expect("insert variant collection");

    for (position, label, username_hint, markdown_content) in [
        (
            0_i32,
            "Alice Variant",
            Some("alice"),
            "Helaragon kommt aus Bandung.",
        ),
        (
            1_i32,
            "Bob Variant",
            Some("bob"),
            "Burgerkill kommt aus Bandung.",
        ),
    ] {
        sqlx::query(
            r#"
            insert into variant_entries (id, collection_id, position, label, username_hint, markdown_content)
            values ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(uuid::Uuid::new_v4())
        .bind(collection_id)
        .bind(position)
        .bind(label)
        .bind(username_hint)
        .bind(markdown_content)
        .execute(pool)
        .await
        .expect("insert variant entry");
    }

    collection_id
}

fn test_config(database_url: String) -> Config {
    Config {
        port: 0,
        database_url,
        session_secret: "test-secret".to_owned(),
        embedding_service_url: "http://127.0.0.1:8001".to_owned(),
    }
}

fn session_cookie_header(headers: &HeaderMap) -> String {
    let raw = headers
        .get(header::SET_COOKIE)
        .expect("set-cookie header")
        .to_str()
        .expect("set-cookie utf8");
    raw.split(';').next().expect("cookie pair").to_owned()
}

#[tokio::test]
async fn patch_section_moves_section_and_rewrites_descendants() {
    let Some(pool) = test_pool().await else {
        eprintln!("Skipping integration test: TEST_DATABASE_URL not set or unreachable");
        return;
    };
    reset_schema(&pool).await;

    let (user_id, session_token) = seed_admin_user(&pool).await;
    let (document_id, parent_a, parent_b, child) = seed_document_tree(&pool, user_id).await;
    let grandchild = seed_nested_descendant(&pool, document_id, child).await;

    let app = app::build(test_config(
        std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL"),
    ))
    .await
    .expect("build app");

    let request = Request::builder()
        .method(Method::PATCH)
        .uri(format!("/api/sections/{child}"))
        .header(header::COOKIE, format!("rimbun_session={session_token}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({
                "title": "Child moved",
                "parent_id": parent_b,
                "position": 0
            })
            .to_string(),
        ))
        .expect("request");

    let response = app.oneshot(request).await.expect("response");
    assert_eq!(response.status(), StatusCode::OK);

    let moved = sqlx::query_as::<_, (Option<uuid::Uuid>, i32, String)>(
        "select parent_id, position, path from sections where id = $1",
    )
    .bind(child)
    .fetch_one(&pool)
    .await
    .expect("load moved section");
    assert_eq!(moved.0, Some(parent_b));
    assert_eq!(moved.1, 0);
    assert_eq!(moved.2, format!("{parent_b}/{child}"));

    let descendant_path =
        sqlx::query_scalar::<_, String>("select path from sections where id = $1")
            .bind(grandchild)
            .fetch_one(&pool)
            .await
            .expect("load grandchild path");
    assert_eq!(descendant_path, format!("{parent_b}/{child}/{grandchild}"));

    let root_positions = sqlx::query_as::<_, (uuid::Uuid, i32)>(
        "select id, position from sections where document_id = $1 and parent_id is null order by position asc",
    )
    .bind(document_id)
    .fetch_all(&pool)
    .await
    .expect("load root positions");
    assert_eq!(root_positions, vec![(parent_a, 0), (parent_b, 1)]);
}

#[tokio::test]
async fn patch_section_rejects_move_into_own_subtree() {
    let Some(pool) = test_pool().await else {
        eprintln!("Skipping integration test: TEST_DATABASE_URL not set or unreachable");
        return;
    };
    reset_schema(&pool).await;

    let (user_id, session_token) = seed_admin_user(&pool).await;
    let (document_id, _parent_a, _parent_b, child) = seed_document_tree(&pool, user_id).await;
    let grandchild = seed_nested_descendant(&pool, document_id, child).await;

    let app = app::build(test_config(
        std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL"),
    ))
    .await
    .expect("build app");

    let request = Request::builder()
        .method(Method::PATCH)
        .uri(format!("/api/sections/{child}"))
        .header(header::COOKIE, format!("rimbun_session={session_token}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({
                "title": "Child",
                "parent_id": grandchild,
                "position": 0
            })
            .to_string(),
        ))
        .expect("request");

    let response = app.oneshot(request).await.expect("response");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read body");
    let text = String::from_utf8(body.to_vec()).expect("utf8 body");
    assert!(text.contains("own subtree"));
}

#[tokio::test]
async fn patch_section_reorders_within_same_parent() {
    let Some(pool) = test_pool().await else {
        eprintln!("Skipping integration test: TEST_DATABASE_URL not set or unreachable");
        return;
    };
    reset_schema(&pool).await;

    let (user_id, session_token) = seed_admin_user(&pool).await;
    let (document_id, parent_a, parent_b, _child) = seed_document_tree(&pool, user_id).await;

    let parent_c = uuid::Uuid::new_v4();
    sqlx::query(
        r#"
        insert into sections (id, document_id, parent_id, title, position, path)
        values ($1, $2, null, $3, $4, $5)
        "#,
    )
    .bind(parent_c)
    .bind(document_id)
    .bind("Parent C")
    .bind(2_i32)
    .bind(parent_c.to_string())
    .execute(&pool)
    .await
    .expect("insert third root section");

    let app = app::build(test_config(
        std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL"),
    ))
    .await
    .expect("build app");

    let request = Request::builder()
        .method(Method::PATCH)
        .uri(format!("/api/sections/{parent_a}"))
        .header(header::COOKIE, format!("rimbun_session={session_token}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({
                "title": "Parent A",
                "parent_id": null,
                "position": 1
            })
            .to_string(),
        ))
        .expect("request");

    let response = app.oneshot(request).await.expect("response");
    assert_eq!(response.status(), StatusCode::OK);

    let root_positions = sqlx::query_as::<_, (uuid::Uuid, i32)>(
        "select id, position from sections where document_id = $1 and parent_id is null order by position asc",
    )
    .bind(document_id)
    .fetch_all(&pool)
    .await
    .expect("load root positions");
    assert_eq!(
        root_positions,
        vec![(parent_b, 0), (parent_a, 1), (parent_c, 2)]
    );
}

#[tokio::test]
async fn auth_register_me_logout_roundtrip_works_via_session_cookie() {
    let Some(pool) = test_pool().await else {
        eprintln!("Skipping integration test: TEST_DATABASE_URL not set or unreachable");
        return;
    };
    reset_schema(&pool).await;

    let app = app::build(test_config(
        std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL"),
    ))
    .await
    .expect("build app");

    let register_request = Request::builder()
        .method(Method::POST)
        .uri("/api/auth/register")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({
                "username": "alice",
                "display_name": "Alice",
                "email": "alice@example.test",
                "password": "correct horse battery staple"
            })
            .to_string(),
        ))
        .expect("register request");

    let register_response = app
        .clone()
        .oneshot(register_request)
        .await
        .expect("register response");
    assert_eq!(register_response.status(), StatusCode::OK);
    let cookie_header = session_cookie_header(register_response.headers());
    let register_body = to_bytes(register_response.into_body(), usize::MAX)
        .await
        .expect("register body");
    let register_json: serde_json::Value =
        serde_json::from_slice(&register_body).expect("register json");
    assert_eq!(register_json["user"]["username"], "alice");
    assert!(register_json["session_token"].is_string());

    let me_request = Request::builder()
        .method(Method::GET)
        .uri("/api/me")
        .header(header::COOKIE, &cookie_header)
        .body(Body::empty())
        .expect("me request");

    let me_response = app.clone().oneshot(me_request).await.expect("me response");
    assert_eq!(me_response.status(), StatusCode::OK);
    let me_body = to_bytes(me_response.into_body(), usize::MAX)
        .await
        .expect("me body");
    let me_json: serde_json::Value = serde_json::from_slice(&me_body).expect("me json");
    assert_eq!(me_json["username"], "alice");
    assert_eq!(me_json["display_name"], "Alice");

    let logout_request = Request::builder()
        .method(Method::POST)
        .uri("/api/auth/logout")
        .header(header::COOKIE, &cookie_header)
        .body(Body::empty())
        .expect("logout request");

    let logout_response = app
        .clone()
        .oneshot(logout_request)
        .await
        .expect("logout response");
    assert_eq!(logout_response.status(), StatusCode::OK);

    let me_after_logout_request = Request::builder()
        .method(Method::GET)
        .uri("/api/me")
        .header(header::COOKIE, &cookie_header)
        .body(Body::empty())
        .expect("me after logout request");

    let me_after_logout_response = app
        .oneshot(me_after_logout_request)
        .await
        .expect("me after logout response");
    assert_eq!(me_after_logout_response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn auth_profile_update_and_password_change_work() {
    let Some(pool) = test_pool().await else {
        eprintln!("Skipping integration test: TEST_DATABASE_URL not set or unreachable");
        return;
    };
    reset_schema(&pool).await;

    let app = app::build(test_config(
        std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL"),
    ))
    .await
    .expect("build app");

    let register_request = Request::builder()
        .method(Method::POST)
        .uri("/api/auth/register")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({
                "username": "bob",
                "display_name": "Bob",
                "email": "bob@example.test",
                "password": "correct horse battery staple"
            })
            .to_string(),
        ))
        .expect("register request");

    let register_response = app
        .clone()
        .oneshot(register_request)
        .await
        .expect("register response");
    assert_eq!(register_response.status(), StatusCode::OK);
    let cookie_header = session_cookie_header(register_response.headers());

    let update_me_request = Request::builder()
        .method(Method::PATCH)
        .uri("/api/me")
        .header(header::COOKIE, &cookie_header)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(json!({ "display_name": "Bobby" }).to_string()))
        .expect("update me request");

    let update_me_response = app
        .clone()
        .oneshot(update_me_request)
        .await
        .expect("update me response");
    assert_eq!(update_me_response.status(), StatusCode::OK);
    let update_me_body = to_bytes(update_me_response.into_body(), usize::MAX)
        .await
        .expect("update me body");
    let update_me_json: serde_json::Value =
        serde_json::from_slice(&update_me_body).expect("update me json");
    assert_eq!(update_me_json["display_name"], "Bobby");

    let change_password_request = Request::builder()
        .method(Method::POST)
        .uri("/api/me/change-password")
        .header(header::COOKIE, &cookie_header)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({
                "current_password": "correct horse battery staple",
                "new_password": "new correct horse battery staple"
            })
            .to_string(),
        ))
        .expect("change password request");

    let change_password_response = app
        .clone()
        .oneshot(change_password_request)
        .await
        .expect("change password response");
    assert_eq!(change_password_response.status(), StatusCode::OK);

    let login_request = Request::builder()
        .method(Method::POST)
        .uri("/api/auth/login")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({
                "identifier": "bob",
                "password": "new correct horse battery staple"
            })
            .to_string(),
        ))
        .expect("login request");

    let login_response = app.oneshot(login_request).await.expect("login response");
    assert_eq!(login_response.status(), StatusCode::OK);
}

#[tokio::test]
async fn me_accepts_session_header_for_account_switching() {
    let Some(pool) = test_pool().await else {
        eprintln!("Skipping integration test: TEST_DATABASE_URL not set or unreachable");
        return;
    };
    reset_schema(&pool).await;

    let (_user_id, session_token) = seed_user_with_role(&pool, "normal").await;

    let app = app::build(test_config(
        std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL"),
    ))
    .await
    .expect("build app");

    let me_request = Request::builder()
        .method(Method::GET)
        .uri("/api/me")
        .header("x-rimbun-session", &session_token)
        .body(Body::empty())
        .expect("me request");

    let me_response = app.oneshot(me_request).await.expect("me response");
    assert_eq!(me_response.status(), StatusCode::OK);
}

#[tokio::test]
async fn admin_can_list_all_users() {
    let Some(pool) = test_pool().await else {
        eprintln!("Skipping integration test: TEST_DATABASE_URL not set or unreachable");
        return;
    };
    reset_schema(&pool).await;

    let (_admin_id, admin_session) = seed_admin_user(&pool).await;
    let (_normal_id, _normal_session) = seed_user_with_role(&pool, "normal").await;

    let app = app::build(test_config(
        std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL"),
    ))
    .await
    .expect("build app");

    let request = Request::builder()
        .method(Method::GET)
        .uri("/api/users")
        .header(header::COOKIE, format!("rimbun_session={admin_session}"))
        .body(Body::empty())
        .expect("request");

    let response = app.oneshot(request).await.expect("response");
    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read body");
    let users: Vec<serde_json::Value> = serde_json::from_slice(&body).expect("users json");
    assert!(users.len() >= 2);
    assert!(users.iter().any(|user| user["role"] == "admin"));
}

#[tokio::test]
async fn admin_can_reset_user_password_and_user_can_login_with_it() {
    let Some(pool) = test_pool().await else {
        eprintln!("Skipping integration test: TEST_DATABASE_URL not set or unreachable");
        return;
    };
    reset_schema(&pool).await;

    let (_admin_id, admin_session) = seed_admin_user(&pool).await;
    let (_normal_id, _normal_session) = seed_user_with_role(&pool, "normal").await;

    let app = app::build(test_config(
        std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL"),
    ))
    .await
    .expect("build app");

    let list_request = Request::builder()
        .method(Method::GET)
        .uri("/api/users")
        .header(header::COOKIE, format!("rimbun_session={admin_session}"))
        .body(Body::empty())
        .expect("list request");

    let list_response = app
        .clone()
        .oneshot(list_request)
        .await
        .expect("list response");
    assert_eq!(list_response.status(), StatusCode::OK);

    let list_body = to_bytes(list_response.into_body(), usize::MAX)
        .await
        .expect("read body");
    let users: Vec<serde_json::Value> = serde_json::from_slice(&list_body).expect("users json");
    let normal_user = users
        .iter()
        .find(|user| user["username"] == "bob")
        .expect("normal user");
    let normal_user_id = normal_user["id"].as_str().expect("user id");

    let reset_request = Request::builder()
        .method(Method::POST)
        .uri(format!("/api/users/{normal_user_id}/reset-password"))
        .header(header::COOKIE, format!("rimbun_session={admin_session}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({
                "new_password": "admin reset correct horse"
            })
            .to_string(),
        ))
        .expect("reset request");

    let reset_response = app
        .clone()
        .oneshot(reset_request)
        .await
        .expect("reset response");
    assert_eq!(reset_response.status(), StatusCode::OK);

    let login_request = Request::builder()
        .method(Method::POST)
        .uri("/api/auth/login")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({
                "identifier": "bob",
                "password": "admin reset correct horse"
            })
            .to_string(),
        ))
        .expect("login request");

    let login_response = app.oneshot(login_request).await.expect("login response");
    assert_eq!(login_response.status(), StatusCode::OK);
}

#[tokio::test]
async fn publish_rebuilds_projection_and_supersedes_previous_submission() {
    let Some(pool) = test_pool().await else {
        eprintln!("Skipping integration test: TEST_DATABASE_URL not set or unreachable");
        return;
    };
    reset_schema(&pool).await;

    let (user_a_id, session_a) = seed_user_with_role(&pool, "normal").await;
    let (_user_b_id, session_b) = seed_user_with_role(&pool, "normal").await;
    let (_document_id, section_id) = seed_single_section_document(&pool, user_a_id).await;

    let app = app::build(test_config(
        std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL"),
    ))
    .await
    .expect("build app");

    sqlx::query(
        r#"
        insert into drafts (
          id,
          section_id,
          user_id,
          base_submission_id,
          markdown_content,
          main_comment_markdown
        )
        values ($1, $2, $3, null, 'Version A1', 'Draft comment')
        "#,
    )
    .bind(uuid::Uuid::new_v4())
    .bind(section_id)
    .bind(user_a_id)
    .execute(&pool)
    .await
    .expect("insert draft before publishing");

    for (cookie, body) in [
        (
            format!("rimbun_session={session_a}"),
            json!({ "base_submission_id": null, "markdown_content": "Version A1" }),
        ),
        (
            format!("rimbun_session={session_b}"),
            json!({ "base_submission_id": null, "markdown_content": "Version B1" }),
        ),
        (
            format!("rimbun_session={session_a}"),
            json!({ "base_submission_id": null, "markdown_content": "Version A2" }),
        ),
    ] {
        let request = Request::builder()
            .method(Method::POST)
            .uri(format!("/api/sections/{section_id}/publish"))
            .header(header::COOKIE, cookie)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))
            .expect("publish request");

        let response = app
            .clone()
            .oneshot(request)
            .await
            .expect("publish response");
        assert_eq!(response.status(), StatusCode::OK);
    }

    let all_submissions = sqlx::query_as::<_, (String, Option<uuid::Uuid>)>(
        r#"
        select markdown_content, superseded_by
        from submissions
        where section_id = $1 and user_id = $2
        order by published_at asc
        "#,
    )
    .bind(section_id)
    .bind(user_a_id)
    .fetch_all(&pool)
    .await
    .expect("load user a submissions");
    assert_eq!(all_submissions.len(), 2);
    assert_eq!(all_submissions[0].0, "Version A1");
    assert!(all_submissions[0].1.is_some());
    assert_eq!(all_submissions[1].0, "Version A2");
    assert!(all_submissions[1].1.is_none());

    let active_submissions = sqlx::query_as::<_, (String,)>(
        r#"
            select markdown_content
            from submissions
            where section_id = $1 and superseded_by is null
            order by published_at desc
            "#,
    )
    .bind(section_id)
    .fetch_all(&pool)
    .await
    .expect("load active submissions");
    assert_eq!(
        active_submissions,
        vec![("Version A2".to_owned(),), ("Version B1".to_owned(),)]
    );

    let remaining_drafts =
        sqlx::query_scalar::<_, i64>("select count(*)::bigint from drafts where section_id = $1")
            .bind(section_id)
            .fetch_one(&pool)
            .await
            .expect("count drafts after publishing");
    assert_eq!(remaining_drafts, 0);

    let projection = sqlx::query_as::<_, (String, i32)>(
        r#"
            select role, rank
            from section_projection_items
            where section_id = $1
            order by rank asc
            "#,
    )
    .bind(section_id)
    .fetch_all(&pool)
    .await
    .expect("load projection");
    assert_eq!(
        projection,
        vec![
            ("main".to_owned(), 0),
            ("principal_alternative".to_owned(), 1),
        ]
    );
}

#[tokio::test]
async fn moderation_hidden_and_soft_deleted_remove_visibility_and_projection() {
    let Some(pool) = test_pool().await else {
        eprintln!("Skipping integration test: TEST_DATABASE_URL not set or unreachable");
        return;
    };
    reset_schema(&pool).await;

    let (moderator_id, moderator_session) = seed_admin_user(&pool).await;
    let (_user_a_id, session_a) = seed_user_with_role(&pool, "normal").await;
    let (_user_b_id, session_b) = seed_user_with_role(&pool, "normal").await;
    let (_document_id, section_id) = seed_single_section_document(&pool, moderator_id).await;

    let app = app::build(test_config(
        std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL"),
    ))
    .await
    .expect("build app");

    for (cookie, content) in [
        (format!("rimbun_session={session_a}"), "Visible A"),
        (format!("rimbun_session={session_b}"), "Visible B"),
    ] {
        let request = Request::builder()
            .method(Method::POST)
            .uri(format!("/api/sections/{section_id}/publish"))
            .header(header::COOKIE, cookie)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({ "base_submission_id": null, "markdown_content": content }).to_string(),
            ))
            .expect("publish request");
        let response = app
            .clone()
            .oneshot(request)
            .await
            .expect("publish response");
        assert_eq!(response.status(), StatusCode::OK);
    }

    let visible_before = sqlx::query_scalar::<_, i64>(
        "select count(*) from section_projection_items where section_id = $1",
    )
    .bind(section_id)
    .fetch_one(&pool)
    .await
    .expect("projection count before");
    assert_eq!(visible_before, 2);

    let submission_b_id = sqlx::query_scalar::<_, uuid::Uuid>(
        "select id from submissions where section_id = $1 and markdown_content = 'Visible B'",
    )
    .bind(section_id)
    .fetch_one(&pool)
    .await
    .expect("submission b");

    let moderate_hidden_request = Request::builder()
        .method(Method::POST)
        .uri(format!("/api/submissions/{submission_b_id}/moderate"))
        .header(
            header::COOKIE,
            format!("rimbun_session={moderator_session}"),
        )
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({
                "hidden": true,
                "soft_deleted": false,
                "excluded_from_clustering": false,
                "reason": "hidden"
            })
            .to_string(),
        ))
        .expect("moderate request");
    let hidden_response = app
        .clone()
        .oneshot(moderate_hidden_request)
        .await
        .expect("moderate hidden response");
    assert_eq!(hidden_response.status(), StatusCode::OK);

    let section_view_request = Request::builder()
        .method(Method::GET)
        .uri(format!("/api/sections/{section_id}/view"))
        .header(
            header::COOKIE,
            format!("rimbun_session={moderator_session}"),
        )
        .body(Body::empty())
        .expect("section view request");
    let section_view_response = app
        .clone()
        .oneshot(section_view_request)
        .await
        .expect("section view response");
    assert_eq!(section_view_response.status(), StatusCode::OK);
    let section_view_body = to_bytes(section_view_response.into_body(), usize::MAX)
        .await
        .expect("section view body");
    let section_view_json: serde_json::Value =
        serde_json::from_slice(&section_view_body).expect("section view json");
    assert_eq!(
        section_view_json["active_submissions"]
            .as_array()
            .expect("active array")
            .len(),
        1
    );
    assert_eq!(
        section_view_json["projection"]
            .as_array()
            .expect("projection array")
            .len(),
        1
    );

    let submission_a_id = sqlx::query_scalar::<_, uuid::Uuid>(
        "select id from submissions where section_id = $1 and markdown_content = 'Visible A'",
    )
    .bind(section_id)
    .fetch_one(&pool)
    .await
    .expect("submission a");

    let moderate_soft_delete_request = Request::builder()
        .method(Method::POST)
        .uri(format!("/api/submissions/{submission_a_id}/moderate"))
        .header(
            header::COOKIE,
            format!("rimbun_session={moderator_session}"),
        )
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({
                "hidden": false,
                "soft_deleted": true,
                "excluded_from_clustering": false,
                "reason": "deleted"
            })
            .to_string(),
        ))
        .expect("soft delete request");
    let deleted_response = app
        .clone()
        .oneshot(moderate_soft_delete_request)
        .await
        .expect("soft delete response");
    assert_eq!(deleted_response.status(), StatusCode::OK);

    let projection_after_delete = sqlx::query_scalar::<_, i64>(
        "select count(*) from section_projection_items where section_id = $1",
    )
    .bind(section_id)
    .fetch_one(&pool)
    .await
    .expect("projection count after delete");
    assert_eq!(projection_after_delete, 0);
}

#[tokio::test]
async fn admin_can_create_and_delete_test_run_from_variant_collection() {
    let Some(pool) = test_pool().await else {
        eprintln!("Skipping integration test: TEST_DATABASE_URL not set or unreachable");
        return;
    };
    reset_schema(&pool).await;

    let (admin_id, admin_session) = seed_admin_user(&pool).await;
    let collection_id = seed_variant_collection(&pool, admin_id).await;

    let app = app::build(test_config(
        std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL"),
    ))
    .await
    .expect("build app");

    let create_request = Request::builder()
        .method(Method::POST)
        .uri(format!(
            "/api/admin/variant-collections/{collection_id}/test-runs"
        ))
        .header(header::COOKIE, format!("rimbun_session={admin_session}"))
        .body(Body::empty())
        .expect("create run request");

    let create_response = app
        .clone()
        .oneshot(create_request)
        .await
        .expect("create run response");
    assert_eq!(create_response.status(), StatusCode::OK);
    let create_body = to_bytes(create_response.into_body(), usize::MAX)
        .await
        .expect("create run body");
    let create_json: serde_json::Value =
        serde_json::from_slice(&create_body).expect("create run json");

    let run_id = uuid::Uuid::parse_str(create_json["run"]["id"].as_str().expect("run id"))
        .expect("run uuid");
    let document_id =
        uuid::Uuid::parse_str(create_json["document"]["id"].as_str().expect("document id"))
            .expect("document uuid");
    let section_id =
        uuid::Uuid::parse_str(create_json["section"]["id"].as_str().expect("section id"))
            .expect("section uuid");
    assert_eq!(create_json["created_users"], 2);

    let run_users_count = sqlx::query_scalar::<_, i64>(
        "select count(*)::bigint from test_run_users where test_run_id = $1",
    )
    .bind(run_id)
    .fetch_one(&pool)
    .await
    .expect("run users count");
    assert_eq!(run_users_count, 2);

    let submissions_count = sqlx::query_scalar::<_, i64>(
        "select count(*)::bigint from submissions where section_id = $1",
    )
    .bind(section_id)
    .fetch_one(&pool)
    .await
    .expect("submissions count");
    assert_eq!(submissions_count, 2);

    let delete_request = Request::builder()
        .method(Method::DELETE)
        .uri(format!("/api/admin/test-runs/{run_id}"))
        .header(header::COOKIE, format!("rimbun_session={admin_session}"))
        .body(Body::empty())
        .expect("delete run request");

    let delete_response = app
        .clone()
        .oneshot(delete_request)
        .await
        .expect("delete run response");
    assert_eq!(delete_response.status(), StatusCode::OK);

    let document_exists =
        sqlx::query_scalar::<_, i64>("select count(*)::bigint from documents where id = $1")
            .bind(document_id)
            .fetch_one(&pool)
            .await
            .expect("document exists");
    assert_eq!(document_exists, 0);

    let remaining_test_users = sqlx::query_scalar::<_, i64>(
        "select count(*)::bigint from test_run_users where test_run_id = $1",
    )
    .bind(run_id)
    .fetch_one(&pool)
    .await
    .expect("remaining run users");
    assert_eq!(remaining_test_users, 0);

    let deleted_status =
        sqlx::query_scalar::<_, String>("select status from test_runs where id = $1")
            .bind(run_id)
            .fetch_one(&pool)
            .await
            .expect("deleted status");
    assert_eq!(deleted_status, "deleted");
}

#[tokio::test]
async fn moderation_excluded_from_clustering_keeps_visibility_but_removes_projection_influence() {
    let Some(pool) = test_pool().await else {
        eprintln!("Skipping integration test: TEST_DATABASE_URL not set or unreachable");
        return;
    };
    reset_schema(&pool).await;

    let (moderator_id, moderator_session) = seed_admin_user(&pool).await;
    let (_user_a_id, session_a) = seed_user_with_role(&pool, "normal").await;
    let (_user_b_id, session_b) = seed_user_with_role(&pool, "normal").await;
    let (_document_id, section_id) = seed_single_section_document(&pool, moderator_id).await;

    let app = app::build(test_config(
        std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL"),
    ))
    .await
    .expect("build app");

    for (cookie, content) in [
        (format!("rimbun_session={session_a}"), "Clustered A"),
        (format!("rimbun_session={session_b}"), "Clustered B"),
    ] {
        let request = Request::builder()
            .method(Method::POST)
            .uri(format!("/api/sections/{section_id}/publish"))
            .header(header::COOKIE, cookie)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({ "base_submission_id": null, "markdown_content": content }).to_string(),
            ))
            .expect("publish request");
        let response = app
            .clone()
            .oneshot(request)
            .await
            .expect("publish response");
        assert_eq!(response.status(), StatusCode::OK);
    }

    let submission_b_id = sqlx::query_scalar::<_, uuid::Uuid>(
        "select id from submissions where section_id = $1 and markdown_content = 'Clustered B'",
    )
    .bind(section_id)
    .fetch_one(&pool)
    .await
    .expect("submission b");

    let moderate_request = Request::builder()
        .method(Method::POST)
        .uri(format!("/api/submissions/{submission_b_id}/moderate"))
        .header(
            header::COOKIE,
            format!("rimbun_session={moderator_session}"),
        )
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({
                "hidden": false,
                "soft_deleted": false,
                "excluded_from_clustering": true,
                "reason": "exclude"
            })
            .to_string(),
        ))
        .expect("moderation request");
    let moderate_response = app
        .clone()
        .oneshot(moderate_request)
        .await
        .expect("moderate response");
    assert_eq!(moderate_response.status(), StatusCode::OK);

    let section_view_request = Request::builder()
        .method(Method::GET)
        .uri(format!("/api/sections/{section_id}/view"))
        .header(
            header::COOKIE,
            format!("rimbun_session={moderator_session}"),
        )
        .body(Body::empty())
        .expect("section view request");
    let section_view_response = app
        .clone()
        .oneshot(section_view_request)
        .await
        .expect("section view response");
    assert_eq!(section_view_response.status(), StatusCode::OK);
    let section_view_body = to_bytes(section_view_response.into_body(), usize::MAX)
        .await
        .expect("section view body");
    let section_view_json: serde_json::Value =
        serde_json::from_slice(&section_view_body).expect("section view json");
    assert_eq!(
        section_view_json["active_submissions"]
            .as_array()
            .expect("active array")
            .len(),
        2
    );
    assert_eq!(
        section_view_json["projection"]
            .as_array()
            .expect("projection array")
            .len(),
        1
    );

    let projected_submission_id = section_view_json["projection"][0]["submission_id"]
        .as_str()
        .expect("projection submission id");
    assert_ne!(projected_submission_id, submission_b_id.to_string());
}

#[tokio::test]
async fn section_compare_requires_auth_for_authenticated_documents() {
    let Some(pool) = test_pool().await else {
        eprintln!("Skipping integration test: TEST_DATABASE_URL not set or unreachable");
        return;
    };
    reset_schema(&pool).await;

    let (owner_id, _owner_session) = seed_admin_user(&pool).await;
    let (_document_id, section_id) = seed_single_section_document(&pool, owner_id).await;

    let app = app::build(test_config(
        std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL"),
    ))
    .await
    .expect("build app");

    let request = Request::builder()
        .method(Method::GET)
        .uri(format!("/api/sections/{section_id}/compare"))
        .body(Body::empty())
        .expect("request");

    let response = app.oneshot(request).await.expect("response");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn section_compare_returns_ranked_block_variants() {
    let Some(pool) = test_pool().await else {
        eprintln!("Skipping integration test: TEST_DATABASE_URL not set or unreachable");
        return;
    };
    reset_schema(&pool).await;

    let (user_a_id, session_a) = seed_user_with_role(&pool, "normal").await;
    let (_user_b_id, session_b) = seed_user_with_role(&pool, "normal").await;
    let (_document_id, section_id) = seed_single_section_document(&pool, user_a_id).await;

    let app = app::build(test_config(
        std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL"),
    ))
    .await
    .expect("build app");

    for (cookie, body) in [
        (
            format!("rimbun_session={session_a}"),
            json!({
                "base_submission_id": null,
                "markdown_content": "# Heading\n\nShared opening paragraph.\n\nA-specific ending.",
                "main_comment_markdown": "Why this version is structured this way."
            }),
        ),
        (
            format!("rimbun_session={session_b}"),
            json!({
                "base_submission_id": null,
                "markdown_content": "# Heading\n\nShared opening paragraph.\n\nB-specific ending.",
                "main_comment_markdown": null
            }),
        ),
    ] {
        let request = Request::builder()
            .method(Method::POST)
            .uri(format!("/api/sections/{section_id}/publish"))
            .header(header::COOKIE, cookie)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))
            .expect("publish request");

        let response = app
            .clone()
            .oneshot(request)
            .await
            .expect("publish response");
        assert_eq!(response.status(), StatusCode::OK);
    }

    let compare_request = Request::builder()
        .method(Method::GET)
        .uri(format!("/api/sections/{section_id}/compare"))
        .header(header::COOKIE, format!("rimbun_session={session_a}"))
        .body(Body::empty())
        .expect("compare request");

    let compare_response = app
        .oneshot(compare_request)
        .await
        .expect("compare response");
    assert_eq!(compare_response.status(), StatusCode::OK);

    let compare_body = to_bytes(compare_response.into_body(), usize::MAX)
        .await
        .expect("compare body");
    let compare_json: serde_json::Value =
        serde_json::from_slice(&compare_body).expect("compare json");

    assert_eq!(compare_json["section_id"], section_id.to_string());
    assert_eq!(compare_json["section_number"], "1");
    assert!(compare_json["main_submission"]["submission_id"].is_string());
    assert!(compare_json["main_submission"]["markdown_content"].is_string());
    assert_eq!(
        compare_json["alternatives"]
            .as_array()
            .expect("alternatives array")
            .len(),
        1
    );
    let comments = compare_json["comments"].as_array().expect("comments array");
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0]["is_primary"], true);
    assert_eq!(
        comments[0]["markdown_content"],
        "Why this version is structured this way."
    );

    let blocks = compare_json["blocks"].as_array().expect("blocks array");
    assert!(!blocks.is_empty());
    assert!(
        blocks
            .iter()
            .any(|block| block["anchor"]["block_path"].is_array())
    );
    assert!(blocks.iter().any(|block| block["variants"].is_array()));
    assert!(blocks.iter().any(|block| {
        block["variants"]
            .as_array()
            .expect("variants array")
            .iter()
            .any(|variant| variant["kind"] == "changed" || variant["kind"] == "unchanged")
    }));
}

#[tokio::test]
async fn authors_and_admins_can_delete_submissions_and_comments() {
    let Some(pool) = test_pool().await else {
        eprintln!("Skipping integration test: TEST_DATABASE_URL not set or unreachable");
        return;
    };
    reset_schema(&pool).await;

    let (admin_id, admin_session) = seed_admin_user(&pool).await;
    let (_author_id, author_session) = seed_user_with_role(&pool, "normal").await;
    let (_other_id, other_session) = seed_user_with_role(&pool, "normal").await;
    let (_document_id, section_id) = seed_single_section_document(&pool, admin_id).await;
    let app = app::build(test_config(
        std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL"),
    ))
    .await
    .expect("build app");

    let publish_request = Request::builder()
        .method(Method::POST)
        .uri(format!("/api/sections/{section_id}/publish"))
        .header(header::COOKIE, format!("rimbun_session={author_session}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({
                "base_submission_id": null,
                "markdown_content": "Contribution to delete",
                "main_comment_markdown": "Author comment"
            })
            .to_string(),
        ))
        .expect("publish request");
    let publish_response = app
        .clone()
        .oneshot(publish_request)
        .await
        .expect("publish response");
    assert_eq!(publish_response.status(), StatusCode::OK);
    let publish_body = to_bytes(publish_response.into_body(), usize::MAX)
        .await
        .expect("publish body");
    let publish_json: serde_json::Value =
        serde_json::from_slice(&publish_body).expect("publish json");
    let submission_id = uuid::Uuid::parse_str(
        publish_json["submission"]["id"]
            .as_str()
            .expect("submission id"),
    )
    .expect("submission uuid");
    let author_comment_id = sqlx::query_scalar::<_, uuid::Uuid>(
        "select id from comments where submission_id = $1 and is_primary = true",
    )
    .bind(submission_id)
    .fetch_one(&pool)
    .await
    .expect("author comment id");

    for uri in [
        format!("/api/submissions/{submission_id}"),
        format!("/api/comments/{author_comment_id}"),
    ] {
        let forbidden_request = Request::builder()
            .method(Method::DELETE)
            .uri(uri)
            .header(header::COOKIE, format!("rimbun_session={other_session}"))
            .body(Body::empty())
            .expect("forbidden delete request");
        let forbidden_response = app
            .clone()
            .oneshot(forbidden_request)
            .await
            .expect("forbidden delete response");
        assert_eq!(forbidden_response.status(), StatusCode::FORBIDDEN);
    }

    let other_comment_request = Request::builder()
        .method(Method::POST)
        .uri(format!("/api/submissions/{submission_id}/comments"))
        .header(header::COOKIE, format!("rimbun_session={other_session}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({
                "parent_comment_id": null,
                "markdown_content": "Admin may delete this"
            })
            .to_string(),
        ))
        .expect("create other comment request");
    let other_comment_response = app
        .clone()
        .oneshot(other_comment_request)
        .await
        .expect("create other comment response");
    assert_eq!(other_comment_response.status(), StatusCode::OK);
    let other_comment_body = to_bytes(other_comment_response.into_body(), usize::MAX)
        .await
        .expect("other comment body");
    let other_comment_json: serde_json::Value =
        serde_json::from_slice(&other_comment_body).expect("other comment json");
    let other_comment_id = other_comment_json["id"].as_str().expect("other comment id");

    let reply_request = Request::builder()
        .method(Method::POST)
        .uri(format!("/api/submissions/{submission_id}/comments"))
        .header(header::COOKIE, format!("rimbun_session={author_session}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({
                "parent_comment_id": other_comment_id,
                "markdown_content": "Reply remains visible"
            })
            .to_string(),
        ))
        .expect("create reply request");
    let reply_response = app
        .clone()
        .oneshot(reply_request)
        .await
        .expect("create reply response");
    assert_eq!(reply_response.status(), StatusCode::OK);
    let reply_body = to_bytes(reply_response.into_body(), usize::MAX)
        .await
        .expect("reply body");
    let reply_json: serde_json::Value = serde_json::from_slice(&reply_body).expect("reply json");
    let reply_id =
        uuid::Uuid::parse_str(reply_json["id"].as_str().expect("reply id")).expect("reply uuid");

    let admin_delete_comment = Request::builder()
        .method(Method::DELETE)
        .uri(format!("/api/comments/{other_comment_id}"))
        .header(header::COOKIE, format!("rimbun_session={admin_session}"))
        .body(Body::empty())
        .expect("admin delete comment request");
    let admin_delete_comment_response = app
        .clone()
        .oneshot(admin_delete_comment)
        .await
        .expect("admin delete comment response");
    assert_eq!(
        admin_delete_comment_response.status(),
        StatusCode::NO_CONTENT
    );
    let deleted_parent = sqlx::query_as::<_, (String, bool)>(
        "select markdown_content, deleted_at is not null from comments where id = $1",
    )
    .bind(uuid::Uuid::parse_str(other_comment_id).expect("other comment uuid"))
    .fetch_one(&pool)
    .await
    .expect("deleted parent state");
    assert_eq!(deleted_parent, (String::new(), true));

    let reply_after_parent_delete =
        sqlx::query_as::<_, (String, Option<chrono::DateTime<chrono::Utc>>)>(
            "select markdown_content, deleted_at from comments where id = $1",
        )
        .bind(reply_id)
        .fetch_one(&pool)
        .await
        .expect("reply after parent delete");
    assert_eq!(reply_after_parent_delete.0, "Reply remains visible");
    assert!(reply_after_parent_delete.1.is_none());

    let author_delete_comment = Request::builder()
        .method(Method::DELETE)
        .uri(format!("/api/comments/{author_comment_id}"))
        .header(header::COOKIE, format!("rimbun_session={author_session}"))
        .body(Body::empty())
        .expect("author delete comment request");
    let author_delete_comment_response = app
        .clone()
        .oneshot(author_delete_comment)
        .await
        .expect("author delete comment response");
    assert_eq!(
        author_delete_comment_response.status(),
        StatusCode::NO_CONTENT
    );

    let replacement_primary_request = Request::builder()
        .method(Method::POST)
        .uri(format!("/api/submissions/{submission_id}/comments"))
        .header(header::COOKIE, format!("rimbun_session={author_session}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({
                "parent_comment_id": null,
                "markdown_content": "Replacement author comment",
                "is_primary": true
            })
            .to_string(),
        ))
        .expect("replacement primary request");
    let replacement_primary_response = app
        .clone()
        .oneshot(replacement_primary_request)
        .await
        .expect("replacement primary response");
    assert_eq!(replacement_primary_response.status(), StatusCode::OK);

    let author_delete_submission = Request::builder()
        .method(Method::DELETE)
        .uri(format!("/api/submissions/{submission_id}"))
        .header(header::COOKIE, format!("rimbun_session={author_session}"))
        .body(Body::empty())
        .expect("author delete submission request");
    let author_delete_submission_response = app
        .clone()
        .oneshot(author_delete_submission)
        .await
        .expect("author delete submission response");
    assert_eq!(
        author_delete_submission_response.status(),
        StatusCode::NO_CONTENT
    );

    let soft_deleted = sqlx::query_scalar::<_, bool>(
        "select soft_deleted from submission_moderation where submission_id = $1",
    )
    .bind(submission_id)
    .fetch_one(&pool)
    .await
    .expect("soft-deleted state");
    assert!(soft_deleted);

    let deleted_comments_request = Request::builder()
        .method(Method::GET)
        .uri(format!("/api/submissions/{submission_id}/comments"))
        .header(header::COOKIE, format!("rimbun_session={author_session}"))
        .body(Body::empty())
        .expect("deleted comments request");
    let deleted_comments_response = app
        .clone()
        .oneshot(deleted_comments_request)
        .await
        .expect("deleted comments response");
    assert_eq!(deleted_comments_response.status(), StatusCode::NOT_FOUND);

    let submissions_request = Request::builder()
        .method(Method::GET)
        .uri(format!("/api/sections/{section_id}/submissions"))
        .header(header::COOKIE, format!("rimbun_session={author_session}"))
        .body(Body::empty())
        .expect("submissions request");
    let submissions_response = app
        .clone()
        .oneshot(submissions_request)
        .await
        .expect("submissions response");
    assert_eq!(submissions_response.status(), StatusCode::OK);
    let submissions_body = to_bytes(submissions_response.into_body(), usize::MAX)
        .await
        .expect("submissions body");
    let visible_submissions: serde_json::Value =
        serde_json::from_slice(&submissions_body).expect("submissions json");
    assert_eq!(
        visible_submissions
            .as_array()
            .expect("submissions array")
            .len(),
        0
    );

    let projection_count = sqlx::query_scalar::<_, i64>(
        "select count(*)::bigint from section_projection_items where section_id = $1",
    )
    .bind(section_id)
    .fetch_one(&pool)
    .await
    .expect("projection count");
    assert_eq!(projection_count, 0);

    let admin_target_publish = Request::builder()
        .method(Method::POST)
        .uri(format!("/api/sections/{section_id}/publish"))
        .header(header::COOKIE, format!("rimbun_session={other_session}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({
                "base_submission_id": null,
                "markdown_content": "Admin deletion target"
            })
            .to_string(),
        ))
        .expect("admin target publish request");
    let admin_target_response = app
        .clone()
        .oneshot(admin_target_publish)
        .await
        .expect("admin target publish response");
    assert_eq!(admin_target_response.status(), StatusCode::OK);
    let admin_target_body = to_bytes(admin_target_response.into_body(), usize::MAX)
        .await
        .expect("admin target body");
    let admin_target_json: serde_json::Value =
        serde_json::from_slice(&admin_target_body).expect("admin target json");
    let admin_target_id = admin_target_json["submission"]["id"]
        .as_str()
        .expect("admin target id");

    let admin_delete_submission = Request::builder()
        .method(Method::DELETE)
        .uri(format!("/api/submissions/{admin_target_id}"))
        .header(header::COOKIE, format!("rimbun_session={admin_session}"))
        .body(Body::empty())
        .expect("admin delete submission request");
    let admin_delete_submission_response = app
        .clone()
        .oneshot(admin_delete_submission)
        .await
        .expect("admin delete submission response");
    assert_eq!(
        admin_delete_submission_response.status(),
        StatusCode::NO_CONTENT
    );
}
