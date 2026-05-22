use axum::{
    body::{to_bytes, Body},
    http::{header, HeaderMap, Method, Request, StatusCode},
};
use rimbun_api::{app, config::Config};
use serde_json::json;
use sqlx::{postgres::PgPoolOptions, PgPool};
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
    sqlx::query("drop schema public cascade; create schema public;")
        .execute(pool)
        .await
        .expect("reset schema");
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

async fn seed_privileged_user(pool: &PgPool) -> (uuid::Uuid, String) {
    seed_user_with_role(pool, "privileged").await
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

async fn seed_nested_descendant(pool: &PgPool, document_id: uuid::Uuid, child: uuid::Uuid) -> uuid::Uuid {
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

    let (user_id, session_token) = seed_privileged_user(&pool).await;
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

    let descendant_path = sqlx::query_scalar::<_, String>("select path from sections where id = $1")
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

    let (user_id, session_token) = seed_privileged_user(&pool).await;
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

    let (user_id, session_token) = seed_privileged_user(&pool).await;
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
    assert_eq!(root_positions, vec![(parent_b, 0), (parent_a, 1), (parent_c, 2)]);
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

    let register_response = app.clone().oneshot(register_request).await.expect("register response");
    assert_eq!(register_response.status(), StatusCode::OK);
    let cookie_header = session_cookie_header(register_response.headers());
    let register_body = to_bytes(register_response.into_body(), usize::MAX)
        .await
        .expect("register body");
    let register_json: serde_json::Value = serde_json::from_slice(&register_body).expect("register json");
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

    let logout_response = app.clone().oneshot(logout_request).await.expect("logout response");
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

    let register_response = app.clone().oneshot(register_request).await.expect("register response");
    assert_eq!(register_response.status(), StatusCode::OK);
    let cookie_header = session_cookie_header(register_response.headers());

    let update_me_request = Request::builder()
        .method(Method::PATCH)
        .uri("/api/me")
        .header(header::COOKIE, &cookie_header)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(json!({ "display_name": "Bobby" }).to_string()))
        .expect("update me request");

    let update_me_response = app.clone().oneshot(update_me_request).await.expect("update me response");
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

    let (_admin_id, admin_session) = seed_privileged_user(&pool).await;
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

        let response = app.clone().oneshot(request).await.expect("publish response");
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

    let active_submissions =
        sqlx::query_as::<_, (String,)>(
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

    let projection =
        sqlx::query_as::<_, (String, i32)>(
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

    let (moderator_id, moderator_session) = seed_privileged_user(&pool).await;
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
        let response = app.clone().oneshot(request).await.expect("publish response");
        assert_eq!(response.status(), StatusCode::OK);
    }

    let visible_before =
        sqlx::query_scalar::<_, i64>("select count(*) from section_projection_items where section_id = $1")
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
        .header(header::COOKIE, format!("rimbun_session={moderator_session}"))
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
        .header(header::COOKIE, format!("rimbun_session={moderator_session}"))
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
    assert_eq!(section_view_json["active_submissions"].as_array().expect("active array").len(), 1);
    assert_eq!(section_view_json["projection"].as_array().expect("projection array").len(), 1);

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
        .header(header::COOKIE, format!("rimbun_session={moderator_session}"))
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

    let projection_after_delete =
        sqlx::query_scalar::<_, i64>("select count(*) from section_projection_items where section_id = $1")
            .bind(section_id)
            .fetch_one(&pool)
            .await
            .expect("projection count after delete");
    assert_eq!(projection_after_delete, 0);
}

#[tokio::test]
async fn moderation_excluded_from_clustering_keeps_visibility_but_removes_projection_influence() {
    let Some(pool) = test_pool().await else {
        eprintln!("Skipping integration test: TEST_DATABASE_URL not set or unreachable");
        return;
    };
    reset_schema(&pool).await;

    let (moderator_id, moderator_session) = seed_privileged_user(&pool).await;
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
        let response = app.clone().oneshot(request).await.expect("publish response");
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
        .header(header::COOKIE, format!("rimbun_session={moderator_session}"))
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
    let moderate_response = app.clone().oneshot(moderate_request).await.expect("moderate response");
    assert_eq!(moderate_response.status(), StatusCode::OK);

    let section_view_request = Request::builder()
        .method(Method::GET)
        .uri(format!("/api/sections/{section_id}/view"))
        .header(header::COOKIE, format!("rimbun_session={moderator_session}"))
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
    assert_eq!(section_view_json["active_submissions"].as_array().expect("active array").len(), 2);
    assert_eq!(section_view_json["projection"].as_array().expect("projection array").len(), 1);

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

    let (owner_id, _owner_session) = seed_privileged_user(&pool).await;
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
                "markdown_content": "# Heading\n\nShared opening paragraph.\n\nA-specific ending."
            }),
        ),
        (
            format!("rimbun_session={session_b}"),
            json!({
                "base_submission_id": null,
                "markdown_content": "# Heading\n\nShared opening paragraph.\n\nB-specific ending."
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

        let response = app.clone().oneshot(request).await.expect("publish response");
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
    assert_eq!(
        compare_json["alternatives"].as_array().expect("alternatives array").len(),
        1
    );

    let blocks = compare_json["blocks"].as_array().expect("blocks array");
    assert!(!blocks.is_empty());
    assert!(blocks.iter().any(|block| block["anchor"]["block_path"].is_array()));
    assert!(blocks.iter().any(|block| block["variants"].is_array()));
    assert!(blocks.iter().any(|block| {
        block["variants"]
            .as_array()
            .expect("variants array")
            .iter()
            .any(|variant| variant["kind"] == "changed" || variant["kind"] == "unchanged")
    }));
}
