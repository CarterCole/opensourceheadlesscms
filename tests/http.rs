use argon2::{password_hash::SaltString, Argon2, PasswordHasher};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use opensourceheadlesscms::{
    cache::{MokaCache, TwoLayerCache},
    config::{
        CacheConfig, CmsConfig, ConfigDirs, DatabaseConfig, DatabaseDriver, FlatfileConfig,
        MediaConfig, RateLimitConfig, ServerConfig,
    },
    core::ContentStore,
    db::SqliteStore,
    http::{build_router, router::RouterConfig},
    rbac::RbacEngine,
    schema::{build_schema, SchemaContext},
};
use rand_core::OsRng;
use serde_json::json;
use std::{collections::HashMap, sync::Arc, time::Duration};
use tower::ServiceExt;

async fn build_test_app() -> (axum::Router, String) {
    let store = SqliteStore::new("sqlite://:memory:", "migrations/sqlite")
        .await
        .unwrap();
    store.run_migrations().await.unwrap();

    // Seed an admin user
    let password = "test-password";
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .unwrap()
        .to_string();
    store
        .create_user("admin@test.com", "Admin", &hash, vec!["admin".into()])
        .await
        .unwrap();

    let store: Arc<dyn ContentStore> = Arc::new(store);

    let cache = Arc::new(TwoLayerCache::new(MokaCache::new(
        1000,
        Duration::from_secs(60),
    )));

    let config = Arc::new(CmsConfig {
        server: ServerConfig {
            http_port: 8080,
            grpc_port: 50051,
            jwt_secret: "test-jwt-secret".into(),
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
        content_types: HashMap::new(),
        workflows: HashMap::new(),
        roles: HashMap::new(),
    });

    let rbac: Arc<dyn opensourceheadlesscms::core::PermissionEngine> =
        Arc::new(RbacEngine::new(&config));

    let events = Arc::new(opensourceheadlesscms::core::EventBus::new());
    let ctx_template =
        SchemaContext::new(store.clone(), cache.clone(), rbac, config.clone(), events);
    let schema = build_schema(&config, ctx_template.clone()).unwrap();

    let router = build_router(RouterConfig {
        store,
        cache,
        schema,
        ctx_template,
        jwt_secret: "test-jwt-secret".into(),
        dev_mode: false,
        admin_ui_dir: None,
        security: Default::default(),
        rate_limit: Default::default(),
        oidc: None,
    });

    (router, password.to_string())
}

async fn login(app: &axum::Router, password: &str) -> String {
    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/login")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({"email": "admin@test.com", "password": password}).to_string(),
        ))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), 65536).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    v["access_token"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn health_returns_200() {
    let (app, _) = build_test_app().await;
    let req = Request::builder()
        .uri("/health")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), 65536).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["status"], "ok");
}

#[tokio::test]
async fn login_wrong_password_returns_401() {
    let (app, _) = build_test_app().await;
    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/login")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({"email": "admin@test.com", "password": "wrong!"}).to_string(),
        ))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn login_correct_password_returns_token() {
    let (app, password) = build_test_app().await;
    let token = login(&app, &password).await;
    assert!(!token.is_empty());
}

#[tokio::test]
async fn graphql_without_auth_returns_error() {
    let (app, _) = build_test_app().await;
    let req = Request::builder()
        .method("POST")
        .uri("/graphql")
        .header("content-type", "application/json")
        .body(Body::from(json!({"query": "{ __typename }"}).to_string()))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    // Either 200 with error payload or 401 — either way the response should indicate auth failure
    let status = resp.status();
    let body = axum::body::to_bytes(resp.into_body(), 65536).await.unwrap();

    if status == StatusCode::OK {
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        // GraphQL error responses have an "errors" key
        assert!(
            v.get("errors").is_some(),
            "unauthenticated graphql should return errors, got: {v}"
        );
    } else {
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }
}

#[tokio::test]
async fn admin_config_requires_auth() {
    let (app, _) = build_test_app().await;
    let req = Request::builder()
        .uri("/api/admin/config")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn admin_config_succeeds_with_auth() {
    let (app, password) = build_test_app().await;
    let token = login(&app, &password).await;

    let req = Request::builder()
        .uri("/api/admin/config")
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), 65536).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(v.get("content_types").is_some());
}
