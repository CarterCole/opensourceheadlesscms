//! Site-wide globals: REST, ES-module export, GraphQL, permissions and storage backends.
use argon2::{password_hash::SaltString, Argon2, PasswordHasher};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use opensourceheadlesscms::{
    cache::{MokaCache, TwoLayerCache},
    config::{
        CmsConfig, ContentPermissions, DatabaseConfig, DatabaseDriver, FlatfileFormat,
        GlobalConfig, GlobalItem, GlobalKind, GlobalStorage, RoleConfig, ServerConfig,
    },
    core::ContentStore,
    db::SqliteStore,
    globals::{FlatfileGlobalStore, GlobalsService},
    http::{build_router, router::RouterConfig},
    rbac::RbacEngine,
    schema::{build_schema, SchemaContext},
};
use rand_core::OsRng;
use serde_json::{json, Value};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tower::ServiceExt;

const PASSWORD: &str = "test-password";

fn global(name: &str, kind: GlobalKind, item: GlobalItem) -> GlobalConfig {
    GlobalConfig {
        name: name.into(),
        slug: None,
        label: None,
        description: None,
        kind,
        item,
        public: true,
        storage: None,
        default: None,
        max_items: None,
    }
}

fn globals() -> HashMap<String, GlobalConfig> {
    let mut site_name = global("site_name", GlobalKind::Single, GlobalItem::Text);
    site_name.default = Some(json!("My Site"));
    let nav = global("primary_nav", GlobalKind::Map, GlobalItem::Url);
    let keywords = global("keywords", GlobalKind::Set, GlobalItem::Text);
    let mut theme = global("theme", GlobalKind::Map, GlobalItem::Json);
    theme.storage = Some(GlobalStorage::Flatfile);
    let mut secret = global("launch_codes", GlobalKind::List, GlobalItem::Text);
    secret.public = false;
    [site_name, nav, keywords, theme, secret]
        .into_iter()
        .map(|g| (g.name.clone(), g))
        .collect()
}

fn role(name: &str, perms: &[(&str, bool)]) -> RoleConfig {
    RoleConfig {
        name: name.into(),
        permissions: perms
            .iter()
            .map(|(res, update)| {
                (
                    res.to_string(),
                    ContentPermissions {
                        create: *update,
                        read: true,
                        update: *update,
                        delete: *update,
                        fields: HashMap::new(),
                    },
                )
            })
            .collect(),
    }
}

async fn build_app(flatfile_dir: &std::path::Path) -> axum::Router {
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
            cache: Default::default(),
            flatfile: Default::default(),
            config_dirs: Default::default(),
            webhooks: vec![],
            rate_limit: Default::default(),
            media: Default::default(),
            security: Default::default(),
            oidc: None,
            globals: Default::default(),
        },
        content_types: HashMap::new(),
        workflows: HashMap::new(),
        roles: HashMap::from([
            ("admin".into(), role("admin", &[("*", true)])),
            // editors can read non-public globals but not change them
            ("editor".into(), role("editor", &[("globals", false)])),
        ]),
        globals: globals(),
    });
    let rbac = Arc::new(RbacEngine::new(&config));
    let events = Arc::new(opensourceheadlesscms::core::EventBus::new());
    let globals = GlobalsService::database(store.clone()).with_backend(
        GlobalStorage::Flatfile,
        Arc::new(FlatfileGlobalStore::new(flatfile_dir, FlatfileFormat::Toml).unwrap()),
    );
    let ctx_template =
        SchemaContext::new(store.clone(), cache.clone(), rbac, config.clone(), events)
            .with_globals(Arc::new(globals));
    let schema = build_schema(&config, ctx_template.clone()).unwrap();
    build_router(RouterConfig {
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
    })
}

async fn login(app: &axum::Router, email: &str) -> String {
    let req = Request::post("/api/auth/login")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({"email": email, "password": PASSWORD}).to_string(),
        ))
        .unwrap();
    let (status, body) = send(app, req).await;
    assert_eq!(status, StatusCode::OK);
    serde_json::from_str::<Value>(&body).unwrap()["access_token"]
        .as_str()
        .unwrap()
        .to_string()
}

async fn send(app: &axum::Router, req: Request<Body>) -> (StatusCode, String) {
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let body = axum::body::to_bytes(resp.into_body(), 1 << 20)
        .await
        .unwrap();
    (status, String::from_utf8(body.to_vec()).unwrap())
}

async fn get(app: &axum::Router, uri: &str, token: Option<&str>) -> (StatusCode, String) {
    let mut req = Request::get(uri);
    if let Some(t) = token {
        req = req.header("authorization", format!("Bearer {t}"));
    }
    send(app, req.body(Body::empty()).unwrap()).await
}

async fn put(
    app: &axum::Router,
    slug: &str,
    value: Value,
    token: Option<&str>,
) -> (StatusCode, String) {
    let mut req = Request::put(format!("/api/admin/globals/{slug}"))
        .header("content-type", "application/json");
    if let Some(t) = token {
        req = req.header("authorization", format!("Bearer {t}"));
    }
    send(
        app,
        req.body(Body::from(json!({ "value": value }).to_string()))
            .unwrap(),
    )
    .await
}

fn parse(body: &str) -> Value {
    serde_json::from_str(body).unwrap()
}

#[tokio::test]
async fn defaults_are_served_before_anything_is_saved() {
    let dir = tempfile::tempdir().unwrap();
    let app = build_app(dir.path()).await;
    let (status, body) = get(&app, "/api/v1/globals", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        parse(&body),
        json!({"keywords": [], "primary-nav": {}, "site-name": "My Site", "theme": {}})
    );
    let (status, body) = get(&app, "/api/v1/globals/site-name", None).await;
    assert_eq!(status, StatusCode::OK);
    let v = parse(&body);
    assert_eq!(v["value"], "My Site");
    assert_eq!(v["kind"], "single");
    assert_eq!(v["updated_at"], Value::Null);
}

#[tokio::test]
async fn admin_saves_and_public_reads_by_slug() {
    let dir = tempfile::tempdir().unwrap();
    let app = build_app(dir.path()).await;
    let admin = login(&app, "admin@test.com").await;

    let (status, body) = put(
        &app,
        "primary-nav",
        json!({"Home": "/", "Blog": "/blog", "GitHub": "https://github.com/example"}),
        Some(&admin),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, _) = put(
        &app,
        "keywords",
        json!(["rust", "cms", "rust"]),
        Some(&admin),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (_, body) = get(&app, "/api/v1/globals/primary-nav", None).await;
    // map order is the editor's order, not alphabetical
    assert!(
        body.contains(
            r#""value":{"Home":"/","Blog":"/blog","GitHub":"https://github.com/example"}"#
        ),
        "{body}"
    );
    let (_, body) = get(&app, "/api/v1/globals/keywords", None).await;
    assert_eq!(parse(&body)["value"], json!(["cms", "rust"]));

    // unknown slug, and lookup by `name` instead of slug, both 404
    assert_eq!(
        get(&app, "/api/v1/globals/nope", None).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        get(&app, "/api/v1/globals/primary_nav", None).await.0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn invalid_values_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let app = build_app(dir.path()).await;
    let admin = login(&app, "admin@test.com").await;
    let (status, body) = put(
        &app,
        "primary-nav",
        json!({"x": "javascript:alert(1)"}),
        Some(&admin),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(parse(&body)["error"].as_str().unwrap().contains("URL"));
    assert_eq!(
        put(
            &app,
            "site-name",
            json!(["not", "a", "string"]),
            Some(&admin)
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        put(&app, "missing", json!("x"), Some(&admin)).await.0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn writes_need_update_permission() {
    let dir = tempfile::tempdir().unwrap();
    let app = build_app(dir.path()).await;
    assert_eq!(
        put(&app, "site-name", json!("x"), None).await.0,
        StatusCode::UNAUTHORIZED
    );
    let editor = login(&app, "editor@test.com").await;
    assert_eq!(
        put(&app, "site-name", json!("x"), Some(&editor)).await.0,
        StatusCode::FORBIDDEN
    );

    let gql = Request::post("/graphql")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {editor}"))
        .body(Body::from(
            json!({"query": r#"mutation { setGlobal(slug: "site-name", value: "x") { value } }"#})
                .to_string(),
        ))
        .unwrap();
    let (_, body) = send(&app, gql).await;
    assert!(body.contains("forbidden"), "{body}");
}

#[tokio::test]
async fn non_public_globals_are_hidden_from_anonymous_callers() {
    let dir = tempfile::tempdir().unwrap();
    let app = build_app(dir.path()).await;
    let admin = login(&app, "admin@test.com").await;
    put(&app, "launch-codes", json!(["0000"]), Some(&admin)).await;

    let (_, body) = get(&app, "/api/v1/globals", None).await;
    assert!(!body.contains("launch"), "{body}");
    assert_eq!(
        get(&app, "/api/v1/globals/launch-codes", None).await.0,
        StatusCode::NOT_FOUND
    );
    let (_, js) = get(&app, "/api/v1/globals.js", None).await;
    assert!(!js.contains("launch"), "{js}");

    let editor = login(&app, "editor@test.com").await;
    let (status, body) = get(&app, "/api/v1/globals/launch-codes", Some(&editor)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(parse(&body)["value"], json!(["0000"]));
}

#[tokio::test]
async fn js_module_export_for_builds() {
    let dir = tempfile::tempdir().unwrap();
    let app = build_app(dir.path()).await;
    let admin = login(&app, "admin@test.com").await;
    put(&app, "primary-nav", json!({"Home": "/"}), Some(&admin)).await;

    let resp = app
        .clone()
        .oneshot(
            Request::get("/api/v1/globals.js")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        resp.headers()["content-type"].to_str().unwrap(),
        "text/javascript; charset=utf-8"
    );
    let js = String::from_utf8(
        axum::body::to_bytes(resp.into_body(), 1 << 20)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(
        js.contains(r#"export const primaryNav = {"Home":"/"};"#),
        "{js}"
    );
    assert!(js.contains(r#"export const siteName = "My Site";"#), "{js}");
    assert!(js.contains(r#"  "primary-nav": primaryNav,"#), "{js}");

    let (status, one) = get(&app, "/api/v1/globals/primary-nav.js", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(one.contains("export const primaryNav"));
    assert!(!one.contains("siteName"));
}

#[tokio::test]
async fn saves_are_visible_immediately_despite_cache() {
    let dir = tempfile::tempdir().unwrap();
    let app = build_app(dir.path()).await;
    let admin = login(&app, "admin@test.com").await;
    let _ = get(&app, "/api/v1/globals", None).await; // warm the cache
    put(&app, "site-name", json!("Renamed"), Some(&admin)).await;
    let (_, body) = get(&app, "/api/v1/globals", None).await;
    assert_eq!(parse(&body)["site-name"], "Renamed");
}

#[tokio::test]
async fn per_global_storage_override_writes_a_flat_file() {
    let dir = tempfile::tempdir().unwrap();
    let app = build_app(dir.path()).await;
    let admin = login(&app, "admin@test.com").await;
    let (status, body) = put(
        &app,
        "theme",
        json!({"light": {"bg": "#fff"}, "dark": {"bg": "#000"}}),
        Some(&admin),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(parse(&body)["storage"], "flatfile");
    let file = std::fs::read_to_string(dir.path().join("theme.toml")).unwrap();
    assert!(file.contains("bg = \"#000\""), "{file}");

    // a hand edit to the file is what gets served (after the cache entry expires/clears)
    std::fs::write(
        dir.path().join("theme.toml"),
        "[value.light]\nbg = \"#eee\"\n",
    )
    .unwrap();
    put(&app, "site-name", json!("bust cache"), Some(&admin)).await;
    let (_, body) = get(&app, "/api/v1/globals/theme", None).await;
    assert_eq!(parse(&body)["value"], json!({"light": {"bg": "#eee"}}));
}

#[tokio::test]
async fn graphql_reads_globals_by_slug() {
    let dir = tempfile::tempdir().unwrap();
    let app = build_app(dir.path()).await;
    let admin = login(&app, "admin@test.com").await;
    let gql = Request::post("/graphql")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {admin}"))
        .body(Body::from(
            json!({
                "query": "mutation($n: String!, $v: JSON!) { setGlobal(name: $n, value: $v) { name value } }",
                "variables": {"n": "keywords", "v": ["b", "a"]},
            })
            .to_string(),
        ))
        .unwrap();
    let (_, body) = send(&app, gql).await;
    assert_eq!(
        parse(&body)["data"]["setGlobal"],
        json!({"name": "keywords", "value": ["a", "b"]}),
        "{body}"
    );

    // GraphQL is signed-in only; anonymous readers use REST.
    let gql = Request::post("/graphql")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {admin}"))
        .body(Body::from(
            json!({"query": r#"{ global(slug: "keywords") { value updatedBy } all: globals { slug name kind item public value } }"#}).to_string(),
        ))
        .unwrap();
    let (_, body) = send(&app, gql).await;
    let v = parse(&body);
    assert_eq!(v["data"]["global"]["value"], json!(["a", "b"]), "{body}");
    assert!(v["data"]["global"]["updatedBy"].is_string());
    let all = v["data"]["all"].as_array().unwrap();
    let site = all.iter().find(|g| g["slug"] == "site-name").unwrap();
    assert_eq!(site["name"], "site_name");
    assert_eq!(site["kind"], "single");
    assert_eq!(site["item"], "text");
    assert_eq!(site["value"], "My Site");
    // admins see non-public globals too
    assert!(all
        .iter()
        .any(|g| g["name"] == "launch_codes" && g["public"] == false));

    // a refused value comes back as a GraphQL error message
    let gql = Request::post("/graphql")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {admin}"))
        .body(Body::from(
            json!({"query": r#"mutation { setGlobal(name: "primary_nav", value: {x: "javascript:1"}) { value } }"#})
                .to_string(),
        ))
        .unwrap();
    let (_, body) = send(&app, gql).await;
    assert!(
        parse(&body)["errors"][0]["message"]
            .as_str()
            .unwrap()
            .contains("URL"),
        "{body}"
    );
}

#[tokio::test]
async fn bulk_reads_can_pick_globals_by_slug_or_name() {
    let dir = tempfile::tempdir().unwrap();
    let app = build_app(dir.path()).await;
    let (_, body) = get(&app, "/api/v1/globals?slugs=site-name,keywords", None).await;
    assert_eq!(
        parse(&body),
        json!({"keywords": [], "site-name": "My Site"})
    );
    let (_, body) = get(
        &app,
        "/api/v1/globals?names=primary_nav&slugs=site-name,nope",
        None,
    )
    .await;
    assert_eq!(
        parse(&body),
        json!({"primary-nav": {}, "site-name": "My Site"})
    );
    // filters never reveal non-public globals to anonymous callers
    let (_, body) = get(&app, "/api/v1/globals?names=launch_codes", None).await;
    assert_eq!(parse(&body), json!({}));
    // the cache keys on the selection: an unfiltered read still returns everything
    let (_, body) = get(&app, "/api/v1/globals", None).await;
    assert_eq!(parse(&body).as_object().unwrap().len(), 4);

    let (_, js) = get(&app, "/api/v1/globals.js?slugs=site-name", None).await;
    assert!(js.contains("export const siteName"), "{js}");
    assert!(!js.contains("primaryNav"), "{js}");
}

async fn graphql(app: &axum::Router, token: &str, query: &str) -> Value {
    let req = Request::post("/graphql")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(json!({ "query": query }).to_string()))
        .unwrap();
    parse(&send(app, req).await.1)
}

#[tokio::test]
async fn graphql_site_has_a_typed_field_per_global() {
    let dir = tempfile::tempdir().unwrap();
    let app = build_app(dir.path()).await;
    let admin = login(&app, "admin@test.com").await;
    put(&app, "keywords", json!(["b", "a"]), Some(&admin)).await;
    put(&app, "primary-nav", json!({"Home": "/"}), Some(&admin)).await;

    let v = graphql(
        &app,
        &admin,
        "{ site { siteName keywords primaryNav theme launchCodes } }",
    )
    .await;
    assert_eq!(
        v["data"]["site"],
        json!({
            "siteName": "My Site",
            "keywords": ["a", "b"],
            "primaryNav": {"Home": "/"},
            "theme": {},
            "launchCodes": [],
        }),
        "{v}"
    );

    // single text is a String and list/set text a [String!], so sub-selections are refused
    let v = graphql(&app, &admin, "{ site { siteName { x } } }").await;
    assert!(v["errors"].is_array(), "{v}");

    // filtered `globals`
    let v = graphql(
        &app,
        &admin,
        r#"{ globals(slugs: ["keywords"], names: ["site_name"]) { slug } }"#,
    )
    .await;
    assert_eq!(
        v["data"]["globals"],
        json!([{"slug": "keywords"}, {"slug": "site-name"}]),
        "{v}"
    );

    // lookup by name still works
    let v = graphql(
        &app,
        &admin,
        r#"{ global(name: "primary_nav") { slug value } }"#,
    )
    .await;
    assert_eq!(
        v["data"]["global"],
        json!({"slug": "primary-nav", "value": {"Home": "/"}})
    );
}
