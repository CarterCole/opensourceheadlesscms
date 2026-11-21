//! Regression tests for authorization on the public REST API and the GraphQL API.
//!
//! Each test pins down a leak or bypass found in the pre-release security audit.

use argon2::{password_hash::SaltString, Argon2, PasswordHasher};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use opensourceheadlesscms::{
    cache::{MokaCache, TwoLayerCache},
    config::{
        CacheConfig, CmsConfig, ConfigDirs, ContentPermissions, ContentTypeConfig, DatabaseConfig,
        DatabaseDriver, FieldAccess, FieldConfig, FieldPermission, FieldType, FlatfileConfig,
        MediaConfig, RateLimitConfig, RoleConfig, SecurityConfig, ServerConfig, StorageBackend,
        Transition, WorkflowConfig,
    },
    core::{ContentData, ContentStore, FieldValue},
    db::SqliteStore,
    http::{build_router, router::RouterConfig},
    rbac::RbacEngine,
    schema::{build_schema, SchemaContext},
};
use rand_core::OsRng;
use serde_json::{json, Value};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tower::ServiceExt;

const PASSWORD: &str = "correct horse battery staple";

fn field(name: &str, access: Option<FieldAccess>) -> FieldConfig {
    FieldConfig {
        name: name.into(),
        field_type: FieldType::Text,
        required: false,
        list: false,
        source: None,
        target: None,
        cardinality: "one".into(),
        access,
        default: None,
        label: None,
    }
}

fn config() -> CmsConfig {
    let mut article = ContentTypeConfig {
        name: "article".into(),
        label: Some("Article".into()),
        storage: StorageBackend::Database,
        fields: vec![
            field("title", None),
            field("slug", None),
            field(
                "internal_notes",
                Some(FieldAccess {
                    read: vec!["admin".into()],
                    write: vec!["admin".into()],
                }),
            ),
        ],
        workflow: Some("editorial".into()),
        searchable: true,
    };
    article.fields[0].required = true; // title
    let workflow = WorkflowConfig {
        name: "editorial".into(),
        states: vec!["draft".into(), "published".into()],
        transitions: vec![Transition {
            from: "draft".into(),
            to: "published".into(),
            roles: vec!["admin".into()],
            trigger: None,
        }],
    };
    let admin = RoleConfig {
        name: "admin".into(),
        permissions: HashMap::from([(
            "*".to_string(),
            ContentPermissions {
                create: true,
                read: true,
                update: true,
                delete: true,
                fields: HashMap::new(),
            },
        )]),
    };
    let editor = RoleConfig {
        name: "editor".into(),
        permissions: HashMap::from([(
            "article".to_string(),
            ContentPermissions {
                create: true,
                read: true,
                update: true,
                delete: false,
                fields: HashMap::from([(
                    "internal_notes".to_string(),
                    FieldPermission {
                        read: false,
                        write: false,
                    },
                )]),
            },
        )]),
    };

    CmsConfig {
        server: ServerConfig {
            http_port: 8080,
            grpc_port: 50051,
            jwt_secret: "test-jwt-secret-that-is-long-enough".into(),
            env: "test".into(),
            database: DatabaseConfig {
                driver: DatabaseDriver::Sqlite,
                url: Some("sqlite://:memory:".into()),
                auto_migrate: false,
                cf_account_id: None,
                cf_database_id: None,
                cf_api_token: None,
            },
            cache: CacheConfig::default(),
            flatfile: FlatfileConfig::default(),
            config_dirs: ConfigDirs::default(),
            webhooks: vec![],
            rate_limit: RateLimitConfig::default(),
            media: MediaConfig::default(),
            security: Default::default(),
            oidc: None,
        },
        content_types: HashMap::from([("article".to_string(), article)]),
        workflows: HashMap::from([("editorial".to_string(), workflow)]),
        roles: HashMap::from([("admin".to_string(), admin), ("editor".to_string(), editor)]),
    }
}

struct App {
    router: axum::Router,
    store: Arc<dyn ContentStore>,
}

async fn app() -> App {
    app_with(SecurityConfig::default(), RateLimitConfig::default()).await
}

async fn app_with(security: SecurityConfig, rate_limit: RateLimitConfig) -> App {
    let store = SqliteStore::new("sqlite://:memory:", "migrations/sqlite")
        .await
        .unwrap();
    store.run_migrations().await.unwrap();

    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(PASSWORD.as_bytes(), &salt)
        .unwrap()
        .to_string();
    store
        .create_user("admin@test.com", "Admin", &hash, vec!["admin".into()])
        .await
        .unwrap();
    store
        .create_user("editor@test.com", "Editor", &hash, vec!["editor".into()])
        .await
        .unwrap();

    let store: Arc<dyn ContentStore> = Arc::new(store);
    let cache = Arc::new(TwoLayerCache::new(MokaCache::new(
        1000,
        Duration::from_secs(60),
    )));
    let config = Arc::new(config());
    let rbac: Arc<dyn opensourceheadlesscms::core::PermissionEngine> =
        Arc::new(RbacEngine::new(&config));
    let events = Arc::new(opensourceheadlesscms::core::EventBus::new());
    let ctx = SchemaContext::new(store.clone(), cache.clone(), rbac, config.clone(), events);
    let schema = build_schema(&config, ctx.clone()).unwrap();

    let router = build_router(RouterConfig {
        store: store.clone(),
        cache,
        schema,
        ctx_template: ctx,
        jwt_secret: config.server.jwt_secret.clone(),
        dev_mode: false,
        admin_ui_dir: None,
        security,
        rate_limit,
        oidc: None,
    });
    App { router, store }
}

async fn seed(app: &App, slug: &str, status: &str, notes: &str) {
    let fields = HashMap::from([
        (
            "title".to_string(),
            FieldValue::Text(format!("Title {slug}")),
        ),
        ("slug".to_string(), FieldValue::Text(slug.to_string())),
        (
            "internal_notes".to_string(),
            FieldValue::Text(notes.to_string()),
        ),
    ]);
    app.store
        .create(
            "article",
            ContentData {
                fields,
                status: Some(status.to_string()),
                scheduled_at: None,
            },
            "seed",
        )
        .await
        .unwrap();
}

async fn get(app: &App, uri: &str, token: Option<&str>) -> (StatusCode, Value, String) {
    let mut req = Request::builder().uri(uri);
    if let Some(t) = token {
        req = req.header("Authorization", format!("Bearer {t}"));
    }
    let resp = app
        .router
        .clone()
        .oneshot(req.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let cache_control = resp
        .headers()
        .get("cache-control")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20)
        .await
        .unwrap();
    let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, json, cache_control)
}

async fn login(app: &App, email: &str) -> String {
    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/login")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({"email": email, "password": PASSWORD}).to_string(),
        ))
        .unwrap();
    let resp = app.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), 65536).await.unwrap();
    let v: Value = serde_json::from_slice(&bytes).unwrap();
    v["access_token"].as_str().unwrap().to_string()
}

async fn gql(app: &App, token: Option<&str>, query: &str) -> Value {
    let mut req = Request::builder()
        .method("POST")
        .uri("/graphql")
        .header("content-type", "application/json");
    if let Some(t) = token {
        req = req.header("Authorization", format!("Bearer {t}"));
    }
    let resp = app
        .router
        .clone()
        .oneshot(
            req.body(Body::from(json!({ "query": query }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

// ---------------------------------------------------------------- public REST

#[tokio::test]
async fn anonymous_list_never_returns_drafts_even_when_asked() {
    let app = app().await;
    seed(&app, "live", "published", "secret").await;
    seed(&app, "wip", "draft", "secret").await;

    for uri in ["/api/v1/article", "/api/v1/article?status=draft"] {
        let (status, body, _) = get(&app, uri, None).await;
        assert_eq!(status, StatusCode::OK);
        let items = body["items"].as_array().unwrap();
        assert!(
            items.iter().all(|i| i["status"] == "published"),
            "{uri} leaked non-published content: {body}"
        );
        assert_eq!(items.len(), 1);
    }
}

#[tokio::test]
async fn anonymous_cannot_fetch_draft_by_slug() {
    let app = app().await;
    seed(&app, "wip", "draft", "secret").await;
    let (status, _, _) = get(&app, "/api/v1/article/wip", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn anonymous_rest_strips_restricted_fields() {
    let app = app().await;
    seed(&app, "live", "published", "TOP-SECRET-NOTE").await;

    let (_, list, _) = get(&app, "/api/v1/article", None).await;
    let (_, one, _) = get(&app, "/api/v1/article/live", None).await;
    let (_, found, _) = get(&app, "/api/v1/search?q=live", None).await;
    for body in [list, one, found] {
        assert!(
            !body.to_string().contains("TOP-SECRET-NOTE"),
            "restricted field leaked publicly: {body}"
        );
    }
}

#[tokio::test]
async fn anonymous_search_excludes_drafts() {
    let app = app().await;
    seed(&app, "wip", "draft", "x").await;
    let (status, body, _) = get(&app, "/api/v1/search?q=wip", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["items"].as_array().unwrap().len(), 0, "{body}");
}

#[tokio::test]
async fn unknown_content_type_is_404() {
    let app = app().await;
    let (status, _, _) = get(&app, "/api/v1/not_a_type", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn authorized_reader_can_see_drafts_but_not_via_shared_cache() {
    let app = app().await;
    seed(&app, "wip", "draft", "admin-only").await;
    let token = login(&app, "admin@test.com").await;

    let (_, body, cc) = get(&app, "/api/v1/article?status=draft", Some(&token)).await;
    assert_eq!(body["items"].as_array().unwrap().len(), 1);
    assert!(
        cc.contains("no-store"),
        "privileged responses must not be cacheable, got {cc:?}"
    );

    // The privileged response must not poison what anonymous callers get next.
    let (_, anon, _) = get(&app, "/api/v1/article?status=draft", None).await;
    assert_eq!(anon["items"].as_array().unwrap().len(), 0);
    let (status, _, _) = get(&app, "/api/v1/article/wip", Some(&token)).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _, _) = get(&app, "/api/v1/article/wip", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// -------------------------------------------------------------------- GraphQL

#[tokio::test]
async fn anonymous_graphql_search_returns_nothing() {
    let app = app().await;
    seed(&app, "wip", "draft", "x").await;
    let v = gql(
        &app,
        None,
        r#"{ search(query: "wip") { id contentType status } }"#,
    )
    .await;
    let hits = v["data"]["search"].as_array().cloned().unwrap_or_default();
    assert!(hits.is_empty(), "anonymous GraphQL search leaked: {v}");
}

#[tokio::test]
async fn anonymous_cannot_transition() {
    let app = app().await;
    seed(&app, "wip", "draft", "x").await;
    let item = app.store.get_by_slug("article", "wip").await.unwrap();
    let v = gql(
        &app,
        None,
        &format!(
            r#"mutation {{ transitionContent(type: "article", id: "{}", toState: "published") {{ status }} }}"#,
            item.id
        ),
    )
    .await;
    assert!(v["errors"].is_array(), "anonymous transition allowed: {v}");
    let after = app.store.get_by_slug("article", "wip").await.unwrap();
    assert_eq!(after.status, "draft");
}

#[tokio::test]
async fn status_cannot_be_set_directly_on_workflow_types() {
    let app = app().await;
    let token = login(&app, "admin@test.com").await;
    let v = gql(
        &app,
        Some(&token),
        r#"mutation { createContent(type: "article", input: {title: "t", slug: "t", status: "published"}) { status } }"#,
    )
    .await;
    assert_eq!(
        v["data"]["createContent"]["status"], "draft",
        "workflow bypassed via create: {v}"
    );
}

#[tokio::test]
async fn workflow_transition_uses_real_current_state() {
    let app = app().await;
    seed(&app, "wip", "draft", "x").await;
    let item = app.store.get_by_slug("article", "wip").await.unwrap();
    let admin = login(&app, "admin@test.com").await;
    let editor = login(&app, "editor@test.com").await;
    let q = format!(
        r#"mutation {{ transitionContent(type: "article", id: "{}", toState: "published") {{ status }} }}"#,
        item.id
    );

    // editor is not in the workflow rule's role list
    let v = gql(&app, Some(&editor), &q).await;
    assert!(v["errors"].is_array(), "editor transitioned: {v}");
    // admin is, and the draft -> published edge now resolves (it never matched before)
    let v = gql(&app, Some(&admin), &q).await;
    assert_eq!(v["data"]["transitionContent"]["status"], "published", "{v}");
}

#[tokio::test]
async fn restricted_field_cannot_be_written_or_read_by_editor() {
    let app = app().await;
    let editor = login(&app, "editor@test.com").await;

    let v = gql(
        &app,
        Some(&editor),
        r#"mutation { createContent(type: "article", input: {title: "t", internalNotes: "pwn"}) { id } }"#,
    )
    .await;
    assert!(
        v["errors"].is_array(),
        "editor wrote a restricted field: {v}"
    );

    seed(&app, "live", "published", "TOP-SECRET-NOTE").await;
    let v = gql(
        &app,
        Some(&editor),
        r#"{ contentList(type: "article") { fields } }"#,
    )
    .await;
    assert!(
        !v.to_string().contains("TOP-SECRET-NOTE"),
        "editor read a restricted field: {v}"
    );
}

#[tokio::test]
async fn unknown_type_cannot_be_created() {
    let app = app().await;
    let admin = login(&app, "admin@test.com").await;
    let v = gql(
        &app,
        Some(&admin),
        r#"mutation { createContent(type: "junk", input: {title: "t"}) { id } }"#,
    )
    .await;
    assert!(v["errors"].is_array(), "created undefined type: {v}");
}

// ----------------------------------------------------------------------- auth

#[tokio::test]
async fn expired_jwt_is_rejected_immediately() {
    use opensourceheadlesscms::http::auth::{decode_jwt, encode_jwt, JwtClaims};
    let claims = JwtClaims {
        sub: "u".into(),
        email: "u@test.com".into(),
        roles: vec![],
        exp: chrono::Utc::now().timestamp() - 5,
        ep: 0,
    };
    let token = encode_jwt(&claims, "s").unwrap();
    assert!(decode_jwt(&token, "s").is_err());
}

#[tokio::test]
async fn unknown_email_and_wrong_password_look_identical() {
    let app = app().await;
    let attempt = |email: &'static str| {
        let router = app.router.clone();
        async move {
            let req = Request::builder()
                .method("POST")
                .uri("/api/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"email": email, "password": "nope"}).to_string(),
                ))
                .unwrap();
            let resp = router.oneshot(req).await.unwrap();
            let status = resp.status();
            let body = axum::body::to_bytes(resp.into_body(), 65536).await.unwrap();
            (status, body)
        }
    };
    assert_eq!(
        attempt("admin@test.com").await,
        attempt("ghost@test.com").await
    );
}

// ------------------------------------------------------- sessions, cookies, abuse

use axum::http::HeaderMap;

async fn send(
    app: &App,
    method: &str,
    uri: &str,
    headers: &[(&str, &str)],
    body: Option<Value>,
) -> (StatusCode, HeaderMap, Value) {
    let mut req = Request::builder().method(method).uri(uri);
    for (k, v) in headers {
        req = req.header(*k, *v);
    }
    let body = match body {
        Some(b) => {
            req = req.header("content-type", "application/json");
            Body::from(b.to_string())
        }
        None => Body::empty(),
    };
    let resp = app
        .router
        .clone()
        .oneshot(req.body(body).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let headers = resp.headers().clone();
    let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20)
        .await
        .unwrap();
    (
        status,
        headers,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn login_raw(app: &App, email: &str) -> (HeaderMap, Value) {
    let (status, headers, body) = send(
        app,
        "POST",
        "/api/auth/login",
        &[],
        Some(json!({"email": email, "password": PASSWORD})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    (headers, body)
}

fn cookie_pair(headers: &HeaderMap) -> String {
    let set_cookie = headers.get("set-cookie").unwrap().to_str().unwrap();
    set_cookie.split(';').next().unwrap().to_string()
}

#[tokio::test]
async fn login_sets_httponly_samesite_cookie_that_authenticates() {
    let app = app().await;
    let (headers, _) = login_raw(&app, "admin@test.com").await;
    let attrs = headers
        .get("set-cookie")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(attrs.contains("HttpOnly"), "{attrs}");
    assert!(attrs.contains("SameSite=Strict"), "{attrs}");
    assert!(
        attrs.contains("Secure"),
        "cookie must be Secure outside dev mode: {attrs}"
    );

    let cookie = cookie_pair(&headers);
    let (status, _, me) = send(&app, "GET", "/api/auth/me", &[("cookie", &cookie)], None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(me["email"], "admin@test.com");
}

#[tokio::test]
async fn cookie_from_another_origin_is_ignored() {
    let app = app().await;
    let (headers, _) = login_raw(&app, "admin@test.com").await;
    let cookie = cookie_pair(&headers);

    let (status, _, _) = send(
        &app,
        "GET",
        "/api/auth/me",
        &[
            ("cookie", &cookie),
            ("host", "cms.test"),
            ("origin", "https://evil.example"),
        ],
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // same origin is fine
    let (status, _, _) = send(
        &app,
        "GET",
        "/api/auth/me",
        &[
            ("cookie", &cookie),
            ("host", "cms.test"),
            ("origin", "https://cms.test"),
        ],
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn logout_revokes_the_token_everywhere() {
    let app = app().await;
    let (headers, body) = login_raw(&app, "admin@test.com").await;
    let token = body["access_token"].as_str().unwrap().to_string();
    let bearer = format!("Bearer {token}");
    let cookie = cookie_pair(&headers);

    let (status, headers, _) = send(
        &app,
        "POST",
        "/api/auth/logout",
        &[("authorization", &bearer)],
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(headers
        .get("set-cookie")
        .unwrap()
        .to_str()
        .unwrap()
        .contains("Max-Age=0"));

    for creds in [
        ("authorization", bearer.as_str()),
        ("cookie", cookie.as_str()),
    ] {
        let (status, _, _) = send(&app, "GET", "/api/auth/me", &[creds], None).await;
        assert_eq!(
            status,
            StatusCode::UNAUTHORIZED,
            "token still valid after logout"
        );
    }
    // logging in again works and yields a fresh, valid session
    let (_, again) = login_raw(&app, "admin@test.com").await;
    let (status, _, _) = send(
        &app,
        "GET",
        "/api/auth/me",
        &[(
            "authorization",
            &format!("Bearer {}", again["access_token"].as_str().unwrap()),
        )],
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn deleted_user_loses_access_immediately() {
    let app = app().await;
    let (_, body) = login_raw(&app, "editor@test.com").await;
    let bearer = format!("Bearer {}", body["access_token"].as_str().unwrap());
    let (status, _, _) = send(
        &app,
        "GET",
        "/api/auth/me",
        &[("authorization", &bearer)],
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let user = app
        .store
        .get_user_by_email("editor@test.com")
        .await
        .unwrap();
    app.store.delete_user(&user.id).await.unwrap();

    let (status, _, _) = send(
        &app,
        "GET",
        "/api/auth/me",
        &[("authorization", &bearer)],
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn role_changes_apply_without_waiting_for_token_expiry() {
    let app = app().await;
    let editor = login(&app, "editor@test.com").await;
    let q = r#"{ contentList(type: "article") { id } }"#;
    let before = gql(&app, Some(&editor), q).await;
    assert!(before["errors"].is_null(), "{before}");

    let user = app
        .store
        .get_user_by_email("editor@test.com")
        .await
        .unwrap();
    app.store.update_user_roles(&user.id, vec![]).await.unwrap();

    let after = gql(&app, Some(&editor), q).await;
    assert!(
        after["errors"].is_array(),
        "stale roles still honoured: {after}"
    );
}

#[tokio::test]
async fn repeated_bad_logins_lock_out_the_client() {
    let app = app().await;
    for _ in 0..5 {
        let (status, _, _) = send(
            &app,
            "POST",
            "/api/auth/login",
            &[],
            Some(json!({"email": "admin@test.com", "password": "wrong"})),
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }
    // even the correct password is refused while locked out
    let (status, headers, _) = send(
        &app,
        "POST",
        "/api/auth/login",
        &[],
        Some(json!({"email": "admin@test.com", "password": PASSWORD})),
    )
    .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert!(headers.contains_key("retry-after"));
}

#[tokio::test]
async fn request_rate_limit_returns_429() {
    let app = app_with(
        SecurityConfig::default(),
        RateLimitConfig {
            public_rps: 1, // burst capacity 2
            graphql_rps: 0,
            auth_rps: 0,
        },
    )
    .await;
    let mut codes = vec![];
    for _ in 0..4 {
        codes.push(send(&app, "GET", "/api/v1/article", &[], None).await.0);
    }
    assert_eq!(codes[0], StatusCode::OK);
    assert!(codes.contains(&StatusCode::TOO_MANY_REQUESTS), "{codes:?}");
    // health checks are exempt
    assert_eq!(
        send(&app, "GET", "/health", &[], None).await.0,
        StatusCode::OK
    );
}

#[tokio::test]
async fn cors_is_same_origin_only_unless_configured() {
    let app = app().await;
    let (_, headers, _) = send(
        &app,
        "GET",
        "/api/v1/article",
        &[("origin", "https://other.example")],
        None,
    )
    .await;
    assert!(!headers.contains_key("access-control-allow-origin"));

    let app = app_with(
        SecurityConfig {
            cors_allowed_origins: vec!["https://site.example".into()],
            ..Default::default()
        },
        RateLimitConfig::default(),
    )
    .await;
    let (_, allowed, _) = send(
        &app,
        "GET",
        "/api/v1/article",
        &[("origin", "https://site.example")],
        None,
    )
    .await;
    assert_eq!(
        allowed.get("access-control-allow-origin").unwrap(),
        "https://site.example"
    );
    let (_, denied, _) = send(
        &app,
        "GET",
        "/api/v1/article",
        &[("origin", "https://other.example")],
        None,
    )
    .await;
    assert!(!denied.contains_key("access-control-allow-origin"));
    assert!(!allowed.contains_key("access-control-allow-credentials"));
}

#[tokio::test]
async fn responses_carry_security_headers() {
    let app = app().await;
    let (_, headers, _) = send(&app, "GET", "/health", &[], None).await;
    assert_eq!(headers.get("x-content-type-options").unwrap(), "nosniff");
    assert_eq!(headers.get("x-frame-options").unwrap(), "DENY");
    assert_eq!(headers.get("referrer-policy").unwrap(), "no-referrer");
}

// ------------------------------------------------------ write validation / authorship

#[tokio::test]
async fn writes_are_validated_against_the_content_type() {
    let app = app().await;
    let admin = login(&app, "admin@test.com").await;
    let bad = [
        // missing required `title`
        r#"mutation { createContent(type: "article", input: {slug: "s"}) { id } }"#,
        // wrong value type
        r#"mutation { createContent(type: "article", input: {title: 5}) { id } }"#,
        // empty required value
        r#"mutation { createContent(type: "article", input: {title: "  "}) { id } }"#,
        // field not in the definition
        r#"mutation { createContent(type: "article", input: {title: "t", bogus: "x"}) { id } }"#,
    ];
    for q in bad {
        let v = gql(&app, Some(&admin), q).await;
        assert!(v["errors"].is_array(), "accepted invalid write {q}: {v}");
    }
    let ok = gql(
        &app,
        Some(&admin),
        r#"mutation { createContent(type: "article", input: {title: "fine", slug: "fine"}) { id } }"#,
    )
    .await;
    assert!(ok["errors"].is_null(), "{ok}");
}

#[tokio::test]
async fn editing_does_not_erase_fields_the_editor_cannot_see() {
    let app = app().await;
    seed(&app, "live", "published", "KEEP-ME").await;
    let item = app.store.get_by_slug("article", "live").await.unwrap();
    let editor = login(&app, "editor@test.com").await;

    // The admin UI sends back only what it could read (no internal notes).
    let v = gql(
        &app,
        Some(&editor),
        &format!(
            r#"mutation {{ updateContent(type: "article", id: "{}", input: {{title: "Renamed", slug: "live"}}) {{ id }} }}"#,
            item.id
        ),
    )
    .await;
    assert!(v["errors"].is_null(), "{v}");

    let after = app.store.get("article", &item.id).await.unwrap();
    assert_eq!(
        after.fields.get("internal_notes"),
        Some(&FieldValue::Text("KEEP-ME".into())),
        "restricted field was wiped by an edit"
    );
    assert_eq!(
        after.fields.get("title"),
        Some(&FieldValue::Text("Renamed".into()))
    );
}

#[tokio::test]
async fn created_by_is_permanent_and_updated_by_tracks_the_last_editor() {
    let app = app().await;
    let admin = app.store.get_user_by_email("admin@test.com").await.unwrap();
    let editor = app
        .store
        .get_user_by_email("editor@test.com")
        .await
        .unwrap();
    let admin_token = login(&app, "admin@test.com").await;
    let editor_token = login(&app, "editor@test.com").await;

    let v = gql(
        &app,
        Some(&admin_token),
        r#"mutation { createContent(type: "article", input: {title: "t", slug: "t"}) { id createdBy updatedBy } }"#,
    )
    .await;
    let id = v["data"]["createContent"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(v["data"]["createContent"]["createdBy"], admin.id);
    assert_eq!(v["data"]["createContent"]["updatedBy"], admin.id);

    let v = gql(
        &app,
        Some(&editor_token),
        &format!(
            r#"mutation {{ updateContent(type: "article", id: "{id}", input: {{title: "t2", slug: "t"}}) {{ createdBy updatedBy }} }}"#
        ),
    )
    .await;
    assert_eq!(v["data"]["updateContent"]["createdBy"], admin.id, "{v}");
    assert_eq!(v["data"]["updateContent"]["updatedBy"], editor.id, "{v}");

    let v = gql(
        &app,
        Some(&admin_token),
        &format!(
            r#"mutation {{ transitionContent(type: "article", id: "{id}", toState: "published") {{ createdBy updatedBy }} }}"#
        ),
    )
    .await;
    assert_eq!(v["data"]["transitionContent"]["createdBy"], admin.id, "{v}");
    assert_eq!(v["data"]["transitionContent"]["updatedBy"], admin.id, "{v}");
}

#[tokio::test]
async fn hostile_search_text_is_treated_as_plain_words() {
    let app = app().await;
    seed(&app, "live", "published", "x").await;
    // FTS5 operators / unbalanced syntax must neither error nor be interpreted.
    for q in [
        "%22",
        "NEAR(",
        "*",
        "a%20OR%20(",
        "title%3Alive",
        "%22%20OR%20%22%22",
    ] {
        let (status, _, _) = get(&app, &format!("/api/v1/search?q={q}"), None).await;
        assert_eq!(status, StatusCode::OK, "search q={q} -> {status}");
    }
    // and an ordinary word still finds the published item
    let (_, body, _) = get(&app, "/api/v1/search?q=live", None).await;
    assert_eq!(body["items"].as_array().unwrap().len(), 1, "{body}");
}
