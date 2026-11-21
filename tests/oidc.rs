//! "Sign in with Kith": the relying-party flow against an in-process mock hub.
//!
//! The mock serves discovery, a JWKS and a token endpoint, and signs whatever ID token
//! claims the test sets. The signing key is a throwaway RSA key in `tests/fixtures`.

use axum::{
    body::Body,
    extract::{Form, State},
    http::{header, HeaderMap, Request, StatusCode},
    routing::{get, post},
    Json, Router,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use opensourceheadlesscms::{
    config::OidcConfig,
    core::ContentStore,
    db::SqliteStore,
    http::{
        auth::authenticate_token,
        oidc::{self, OidcClient, OidcState},
    },
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tower::ServiceExt;

const KEY_PEM: &str = include_str!("fixtures/oidc_test_rsa_key.pem");
const KEY_N: &str = include_str!("fixtures/oidc_test_rsa_n.txt");
const SECRET: &str = "test-jwt-secret-test-jwt-secret-1234";
const CLIENT_ID: &str = "cms";

// ---------------------------------------------------------------------------------
// Mock hub
// ---------------------------------------------------------------------------------

#[derive(Default)]
struct Hub {
    issuer: Mutex<String>,
    /// Claims the token endpoint signs next.
    claims: Mutex<Value>,
    /// Sent verbatim instead of a signed token, when set.
    raw_id_token: Mutex<Option<String>>,
    last_form: Mutex<Option<HashMap<String, String>>>,
    last_authorization: Mutex<Option<String>>,
}

async fn discovery(State(hub): State<Arc<Hub>>) -> Json<Value> {
    let iss = hub.issuer.lock().unwrap().clone();
    Json(json!({
        "issuer": iss,
        "authorization_endpoint": format!("{iss}/oauth/authorize"),
        "token_endpoint": format!("{iss}/oauth/token"),
        "jwks_uri": format!("{iss}/.well-known/jwks.json"),
        "code_challenge_methods_supported": ["S256"],
    }))
}

async fn jwks() -> Json<Value> {
    Json(json!({"keys": [{
        "kty": "RSA", "kid": "k1", "alg": "RS256", "use": "sig",
        "n": KEY_N.trim(), "e": "AQAB"
    }]}))
}

async fn token(
    State(hub): State<Arc<Hub>>,
    headers: HeaderMap,
    Form(form): Form<HashMap<String, String>>,
) -> Json<Value> {
    *hub.last_form.lock().unwrap() = Some(form);
    *hub.last_authorization.lock().unwrap() = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let id_token = match hub.raw_id_token.lock().unwrap().clone() {
        Some(t) => t,
        None => {
            let mut h = Header::new(Algorithm::RS256);
            h.kid = Some("k1".into());
            encode(
                &h,
                &*hub.claims.lock().unwrap(),
                &EncodingKey::from_rsa_pem(KEY_PEM.as_bytes()).unwrap(),
            )
            .unwrap()
        }
    };
    Json(json!({"id_token": id_token, "access_token": "unused", "token_type": "Bearer"}))
}

async fn start_hub() -> (Arc<Hub>, String) {
    let hub = Arc::new(Hub::default());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let issuer = format!("http://{}", listener.local_addr().unwrap());
    *hub.issuer.lock().unwrap() = issuer.clone();
    let app = Router::new()
        .route("/.well-known/openid-configuration", get(discovery))
        .route("/.well-known/jwks.json", get(jwks))
        .route("/oauth/token", post(token))
        .with_state(hub.clone());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (hub, issuer)
}

// ---------------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------------

struct Env {
    app: Router,
    store: Arc<dyn ContentStore>,
    hub: Arc<Hub>,
    issuer: String,
}

fn config(issuer: &str) -> OidcConfig {
    OidcConfig {
        issuer: issuer.to_string(),
        client_id: CLIENT_ID.into(),
        client_secret: None,
        name: "Kith".into(),
        public_url: Some("https://cms.example.com".into()),
        redirect_uri: None,
        scopes: vec!["openid".into(), "email".into(), "groups".into()],
        min_aal: None,
        auto_create: false,
        default_roles: vec!["editor".into()],
        role_map: Default::default(),
        post_login_url: "/admin/".into(),
    }
}

async fn env_with(tweak: impl FnOnce(&mut OidcConfig)) -> Env {
    let (hub, issuer) = start_hub().await;
    let store = SqliteStore::new("sqlite://:memory:", "migrations/sqlite")
        .await
        .unwrap();
    store.run_migrations().await.unwrap();
    store
        .create_user("ada@example.com", "Ada", "$unused", vec!["editor".into()])
        .await
        .unwrap();
    let store: Arc<dyn ContentStore> = Arc::new(store);

    let mut cfg = config(&issuer);
    tweak(&mut cfg);
    let app = oidc::router(OidcState {
        client: Some(Arc::new(OidcClient::new(cfg).unwrap())),
        store: store.clone(),
        jwt_secret: SECRET.into(),
        jwt_ttl_seconds: 3600,
        cookie_secure: false,
    });
    Env {
        app,
        store,
        hub,
        issuer,
    }
}

async fn env() -> Env {
    env_with(|_| {}).await
}

/// What the login redirect told the provider, and the cookie that binds the browser.
struct Started {
    state: String,
    nonce: String,
    challenge: String,
    query: HashMap<String, String>,
    cookie: String,
}

fn set_cookies(resp: &axum::response::Response) -> Vec<String> {
    resp.headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .map(|v| v.to_str().unwrap().to_string())
        .collect()
}

fn cookie_pair(resp: &axum::response::Response, name: &str) -> Option<String> {
    set_cookies(resp)
        .into_iter()
        .find(|c| c.starts_with(&format!("{name}=")))
        .map(|c| c.split(';').next().unwrap().to_string())
}

async fn start(env: &Env) -> Started {
    let resp = env
        .app
        .clone()
        .oneshot(
            Request::get("/api/auth/oidc/login")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::SEE_OTHER);
    let location = resp.headers()[header::LOCATION]
        .to_str()
        .unwrap()
        .to_string();
    let url = url::Url::parse(&location).unwrap();
    assert_eq!(url.path(), "/oauth/authorize");
    let query: HashMap<String, String> = url.query_pairs().into_owned().collect();

    // The state cookie must not be readable by scripts and must survive the provider's
    // cross-site redirect back.
    let raw = set_cookies(&resp)
        .into_iter()
        .find(|c| c.starts_with("cms_oidc="))
        .unwrap();
    assert!(
        raw.contains("HttpOnly") && raw.contains("SameSite=Lax"),
        "{raw}"
    );

    Started {
        state: query["state"].clone(),
        nonce: query["nonce"].clone(),
        challenge: query["code_challenge"].clone(),
        cookie: cookie_pair(&resp, "cms_oidc").unwrap(),
        query,
    }
}

fn claims(env: &Env, nonce: &str, extra: Value) -> Value {
    let now = chrono::Utc::now().timestamp();
    let mut c = json!({
        "iss": env.issuer, "sub": "did:kith:ada", "aud": CLIENT_ID, "azp": CLIENT_ID,
        "iat": now, "exp": now + 300, "nonce": nonce,
        "email": "ada@example.com", "email_verified": true,
        "name": "Ada Lovelace", "acr": "urn:kith:acr:aal2",
    });
    for (k, v) in extra.as_object().unwrap() {
        if v.is_null() {
            c.as_object_mut().unwrap().remove(k);
        } else {
            c[k] = v.clone();
        }
    }
    c
}

async fn callback_raw(env: &Env, uri: &str, cookie: Option<&str>) -> axum::response::Response {
    let mut req = Request::get(uri);
    if let Some(c) = cookie {
        req = req.header(header::COOKIE, c);
    }
    env.app
        .clone()
        .oneshot(req.body(Body::empty()).unwrap())
        .await
        .unwrap()
}

/// Run the whole flow with `extra` merged over the default ID token claims.
async fn sign_in(env: &Env, extra: Value) -> axum::response::Response {
    let s = start(env).await;
    *env.hub.claims.lock().unwrap() = claims(env, &s.nonce, extra);
    callback_raw(
        env,
        &format!(
            "/api/auth/oidc/callback?code=abc&state={}&iss={}",
            s.state, env.issuer
        ),
        Some(&s.cookie),
    )
    .await
}

fn assert_rejected(resp: &axum::response::Response) {
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    assert!(
        cookie_pair(resp, "cms_session").is_none(),
        "no session may be issued"
    );
}

// ---------------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------------

#[tokio::test]
async fn existing_user_signs_in_and_gets_a_normal_session() {
    let env = env().await;
    let s = start(&env).await;

    // What we asked the provider for.
    assert_eq!(s.query["response_type"], "code");
    assert_eq!(s.query["client_id"], CLIENT_ID);
    assert_eq!(
        s.query["redirect_uri"],
        "https://cms.example.com/api/auth/oidc/callback"
    );
    assert_eq!(s.query["scope"], "openid email groups");
    assert_eq!(s.query["code_challenge_method"], "S256");
    assert!(!s.query.contains_key("acr_values"));

    *env.hub.claims.lock().unwrap() = claims(&env, &s.nonce, json!({}));
    let resp = callback_raw(
        &env,
        &format!(
            "/api/auth/oidc/callback?code=abc&state={}&iss={}",
            s.state, env.issuer
        ),
        Some(&s.cookie),
    )
    .await;

    assert_eq!(resp.status(), StatusCode::SEE_OTHER);
    assert_eq!(resp.headers()[header::LOCATION], "/admin/");
    let session = cookie_pair(&resp, "cms_session").expect("session cookie");
    let raw = set_cookies(&resp)
        .into_iter()
        .find(|c| c.starts_with("cms_session="))
        .unwrap();
    assert!(
        raw.contains("HttpOnly") && raw.contains("SameSite=Strict"),
        "{raw}"
    );
    // The one-time state cookie is cleared.
    assert!(set_cookies(&resp)
        .iter()
        .any(|c| c.starts_with("cms_oidc=;") && c.contains("Max-Age=0")));

    // It is the same kind of token the password login issues.
    let token = session.strip_prefix("cms_session=").unwrap();
    let (user_id, roles) = authenticate_token(env.store.as_ref(), SECRET, token)
        .await
        .expect("valid session");
    assert_eq!(
        user_id,
        env.store
            .get_user_by_email("ada@example.com")
            .await
            .unwrap()
            .id
    );
    assert_eq!(roles, vec!["editor".to_string()]);

    // The code was redeemed with the PKCE verifier that matches the challenge, and a
    // public client names itself in the body.
    let form = env.hub.last_form.lock().unwrap().clone().unwrap();
    assert_eq!(form["grant_type"], "authorization_code");
    assert_eq!(form["code"], "abc");
    assert_eq!(form["redirect_uri"], s.query["redirect_uri"]);
    assert_eq!(form["client_id"], CLIENT_ID);
    assert_eq!(
        URL_SAFE_NO_PAD.encode(Sha256::digest(form["code_verifier"].as_bytes())),
        s.challenge
    );
    assert!(env.hub.last_authorization.lock().unwrap().is_none());
}

#[tokio::test]
async fn confidential_client_authenticates_with_basic() {
    let env = env_with(|c| c.client_secret = Some("s3cret".into())).await;
    let resp = sign_in(&env, json!({})).await;
    assert_eq!(resp.status(), StatusCode::SEE_OTHER);

    let basic = format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode("cms:s3cret")
    );
    assert_eq!(
        env.hub.last_authorization.lock().unwrap().as_deref(),
        Some(basic.as_str())
    );
    assert!(!env
        .hub
        .last_form
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .contains_key("client_id"));
}

#[tokio::test]
async fn email_match_ignores_case_of_the_stored_account() {
    let env = env().await;
    env.store
        .create_user(
            "Grace@Example.com",
            "Grace",
            "$unused",
            vec!["editor".into()],
        )
        .await
        .unwrap();
    let resp = sign_in(&env, json!({"email": "grace@example.com"})).await;
    assert_eq!(resp.status(), StatusCode::SEE_OTHER);
}

#[tokio::test]
async fn accounts_that_differ_only_by_case_are_not_guessed_between() {
    let env = env().await;
    for e in ["Dup@example.com", "DUP@example.com"] {
        env.store
            .create_user(e, "Dup", "$unused", vec!["admin".into()])
            .await
            .unwrap();
    }
    assert_rejected(&sign_in(&env, json!({"email": "dup@example.com"})).await);
    // An exact match still wins.
    assert_eq!(
        sign_in(&env, json!({"email": "DUP@example.com"}))
            .await
            .status(),
        StatusCode::SEE_OTHER
    );
}

#[tokio::test]
async fn unknown_email_is_refused_by_default() {
    let env = env().await;
    let resp = sign_in(&env, json!({"email": "mallory@example.com"})).await;
    assert_rejected(&resp);
    assert!(env
        .store
        .get_user_by_email("mallory@example.com")
        .await
        .is_err());
}

#[tokio::test]
async fn auto_create_makes_an_account_that_has_no_usable_password() {
    let env = env_with(|c| c.auto_create = true).await;
    let resp = sign_in(
        &env,
        json!({"email": "new@example.com", "name": "New Person"}),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::SEE_OTHER);

    let user = env
        .store
        .get_user_by_email("new@example.com")
        .await
        .unwrap();
    assert_eq!(user.display_name, "New Person");
    assert_eq!(user.roles, vec!["editor".to_string()]);
    let hash = env
        .store
        .get_password_hash("new@example.com")
        .await
        .unwrap();
    assert!(
        hash.starts_with("$argon2"),
        "a real hash of a value nobody knows: {hash}"
    );
}

#[tokio::test]
async fn unverified_or_missing_email_is_refused() {
    let env = env().await;
    assert_rejected(&sign_in(&env, json!({"email_verified": false})).await);
    assert_rejected(&sign_in(&env, json!({"email_verified": null})).await);
    assert_rejected(&sign_in(&env, json!({"email": null})).await);
}

#[tokio::test]
async fn role_map_makes_the_provider_the_source_of_roles() {
    let env = env_with(|c| {
        c.role_map = [("cms-admins".to_string(), "admin".to_string())].into();
    })
    .await;

    // A group the map knows promotes, from `groups` or from `roles`.
    let resp = sign_in(&env, json!({"groups": ["cms-admins", "other"]})).await;
    assert_eq!(resp.status(), StatusCode::SEE_OTHER);
    let user = env
        .store
        .get_user_by_email("ada@example.com")
        .await
        .unwrap();
    assert_eq!(user.roles, vec!["admin".to_string()]);

    // Losing the group demotes to the defaults on the next sign-in.
    let resp = sign_in(&env, json!({"groups": ["other"]})).await;
    assert_eq!(resp.status(), StatusCode::SEE_OTHER);
    let user = env
        .store
        .get_user_by_email("ada@example.com")
        .await
        .unwrap();
    assert_eq!(user.roles, vec!["editor".to_string()]);
}

#[tokio::test]
async fn without_a_role_map_existing_roles_are_left_alone() {
    let env = env().await;
    let resp = sign_in(&env, json!({"groups": ["cms-admins"], "roles": ["admin"]})).await;
    assert_eq!(resp.status(), StatusCode::SEE_OTHER);
    let user = env
        .store
        .get_user_by_email("ada@example.com")
        .await
        .unwrap();
    assert_eq!(user.roles, vec!["editor".to_string()]);
}

#[tokio::test]
async fn min_aal_is_requested_and_enforced() {
    let env = env_with(|c| c.min_aal = Some(2)).await;
    let s = start(&env).await;
    assert_eq!(s.query["acr_values"], "urn:kith:acr:aal2");

    assert_eq!(
        sign_in(&env, json!({"acr": "urn:kith:acr:aal2"}))
            .await
            .status(),
        StatusCode::SEE_OTHER
    );
    assert_eq!(
        sign_in(&env, json!({"acr": "urn:kith:acr:aal3"}))
            .await
            .status(),
        StatusCode::SEE_OTHER
    );
    assert_rejected(&sign_in(&env, json!({"acr": "urn:kith:acr:aal1"})).await);
    assert_rejected(&sign_in(&env, json!({"acr": null})).await);
}

#[tokio::test]
async fn bad_id_tokens_are_refused() {
    let env = env().await;
    let now = chrono::Utc::now().timestamp();
    assert_rejected(&sign_in(&env, json!({"nonce": "not-ours"})).await);
    assert_rejected(&sign_in(&env, json!({"nonce": null})).await);
    assert_rejected(&sign_in(&env, json!({"aud": "someone-else"})).await);
    assert_rejected(&sign_in(&env, json!({"azp": "someone-else"})).await);
    assert_rejected(&sign_in(&env, json!({"iss": "https://evil.example"})).await);
    assert_rejected(&sign_in(&env, json!({"exp": now - 3600})).await);
    assert_rejected(&sign_in(&env, json!({"sub": null})).await);
}

#[tokio::test]
async fn only_rs256_is_accepted() {
    let env = env().await;
    let s = start(&env).await;
    // Algorithm confusion: an HS256 token keyed with the public modulus.
    let forged = encode(
        &Header::new(Algorithm::HS256),
        &claims(&env, &s.nonce, json!({})),
        &EncodingKey::from_secret(KEY_N.trim().as_bytes()),
    )
    .unwrap();
    *env.hub.raw_id_token.lock().unwrap() = Some(forged);
    let resp = callback_raw(
        &env,
        &format!("/api/auth/oidc/callback?code=abc&state={}", s.state),
        Some(&s.cookie),
    )
    .await;
    assert_rejected(&resp);

    // And `alg: none`.
    let none = format!(
        "{}.{}.",
        URL_SAFE_NO_PAD.encode(r#"{"alg":"none","typ":"JWT"}"#),
        URL_SAFE_NO_PAD.encode(claims(&env, &s.nonce, json!({})).to_string())
    );
    *env.hub.raw_id_token.lock().unwrap() = Some(none);
    let resp = callback_raw(
        &env,
        &format!("/api/auth/oidc/callback?code=abc&state={}", s.state),
        Some(&s.cookie),
    )
    .await;
    assert_rejected(&resp);
}

#[tokio::test]
async fn a_token_signed_by_another_key_is_refused() {
    let env = env().await;
    let s = start(&env).await;
    // Same kid, different key material: signature check must fail.
    let other = std::process::Command::new("openssl")
        .args([
            "genpkey",
            "-algorithm",
            "RSA",
            "-pkeyopt",
            "rsa_keygen_bits:2048",
        ])
        .output();
    let Ok(other) = other else { return }; // no openssl here: nothing to forge with
    let pem = String::from_utf8(other.stdout).unwrap();
    let mut h = Header::new(Algorithm::RS256);
    h.kid = Some("k1".into());
    let forged = encode(
        &h,
        &claims(&env, &s.nonce, json!({})),
        &EncodingKey::from_rsa_pem(pem.as_bytes()).unwrap(),
    )
    .unwrap();
    *env.hub.raw_id_token.lock().unwrap() = Some(forged);
    let resp = callback_raw(
        &env,
        &format!("/api/auth/oidc/callback?code=abc&state={}", s.state),
        Some(&s.cookie),
    )
    .await;
    assert_rejected(&resp);
}

#[tokio::test]
async fn callback_must_come_from_the_browser_that_started_the_flow() {
    let env = env().await;
    let s = start(&env).await;
    *env.hub.claims.lock().unwrap() = claims(&env, &s.nonce, json!({}));
    let uri = format!("/api/auth/oidc/callback?code=abc&state={}", s.state);

    // No cookie (an attacker's link opened in the victim's browser, or another browser).
    assert_rejected(&callback_raw(&env, &uri, None).await);
    // A cookie that is not ours.
    assert_rejected(&callback_raw(&env, &uri, Some("cms_oidc=garbage")).await);
    // A cookie signed under the session secret's own key, not the derived one.
    let session_like = encode(
        &Header::default(),
        &json!({"typ": "oidc-state", "state": s.state, "nonce": s.nonce, "verifier": "v", "exp": chrono::Utc::now().timestamp() + 60}),
        &EncodingKey::from_secret(SECRET.as_bytes()),
    )
    .unwrap();
    assert_rejected(&callback_raw(&env, &uri, Some(&format!("cms_oidc={session_like}"))).await);
    // Right cookie, wrong or missing state.
    assert_rejected(
        &callback_raw(
            &env,
            "/api/auth/oidc/callback?code=abc&state=other",
            Some(&s.cookie),
        )
        .await,
    );
    assert_rejected(&callback_raw(&env, "/api/auth/oidc/callback?code=abc", Some(&s.cookie)).await);
    // None of that reached the token endpoint.
    assert!(env.hub.last_form.lock().unwrap().is_none());
}

#[tokio::test]
async fn provider_errors_and_wrong_issuer_parameter_are_refused() {
    let env = env().await;
    let s = start(&env).await;
    *env.hub.claims.lock().unwrap() = claims(&env, &s.nonce, json!({}));

    let denied = format!(
        "/api/auth/oidc/callback?error=access_denied&state={}",
        s.state
    );
    assert_rejected(&callback_raw(&env, &denied, Some(&s.cookie)).await);

    // RFC 9207: a response claiming to be from another issuer (mix-up).
    let mixup = format!(
        "/api/auth/oidc/callback?code=abc&state={}&iss=https%3A%2F%2Fevil.example",
        s.state
    );
    assert_rejected(&callback_raw(&env, &mixup, Some(&s.cookie)).await);
    assert!(env.hub.last_form.lock().unwrap().is_none());
}

#[tokio::test]
async fn unreachable_provider_is_a_bad_gateway_not_a_session() {
    // Nothing listens on this port.
    let store = SqliteStore::new("sqlite://:memory:", "migrations/sqlite")
        .await
        .unwrap();
    store.run_migrations().await.unwrap();
    let app = oidc::router(OidcState {
        client: Some(Arc::new(
            OidcClient::new(config("http://127.0.0.1:9")).unwrap(),
        )),
        store: Arc::new(store),
        jwt_secret: SECRET.into(),
        jwt_ttl_seconds: 3600,
        cookie_secure: false,
    });
    let resp = app
        .oneshot(
            Request::get("/api/auth/oidc/login")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_GATEWAY);
    assert!(!resp.headers().contains_key(header::LOCATION));
}

#[tokio::test]
async fn routes_are_inert_when_oidc_is_not_configured() {
    let store = SqliteStore::new("sqlite://:memory:", "migrations/sqlite")
        .await
        .unwrap();
    let app = oidc::router(OidcState {
        client: None,
        store: Arc::new(store),
        jwt_secret: SECRET.into(),
        jwt_ttl_seconds: 3600,
        cookie_secure: false,
    });
    let get = |uri: &'static str| Request::get(uri).body(Body::empty()).unwrap();

    let resp = app
        .clone()
        .oneshot(get("/api/auth/oidc/config"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value =
        serde_json::from_slice(&axum::body::to_bytes(resp.into_body(), 4096).await.unwrap())
            .unwrap();
    assert_eq!(body, json!({"enabled": false}));

    for uri in [
        "/api/auth/oidc/login",
        "/api/auth/oidc/callback?code=x&state=y",
    ] {
        assert_eq!(
            app.clone().oneshot(get(uri)).await.unwrap().status(),
            StatusCode::NOT_FOUND
        );
    }
}

#[tokio::test]
async fn config_endpoint_reports_the_button_label() {
    let env = env_with(|c| c.name = "Acme ID".into()).await;
    let resp = env
        .app
        .clone()
        .oneshot(
            Request::get("/api/auth/oidc/config")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body: Value =
        serde_json::from_slice(&axum::body::to_bytes(resp.into_body(), 4096).await.unwrap())
            .unwrap();
    assert_eq!(body, json!({"enabled": true, "name": "Acme ID"}));
}

#[test]
fn config_validation() {
    let ok = config("https://id.example.com");
    assert!(ok.validate().is_ok());
    assert_eq!(
        ok.redirect_uri().as_deref(),
        Some("https://cms.example.com/api/auth/oidc/callback")
    );

    // Loopback http is fine for development; any other http is not.
    let mut c = config("http://localhost:7788");
    c.public_url = Some("http://localhost:8080/".into());
    assert!(c.validate().is_ok());
    assert_eq!(
        c.redirect_uri().as_deref(),
        Some("http://localhost:8080/api/auth/oidc/callback")
    );
    assert!(config("http://id.example.com").validate().is_err());
    assert!(config("ftp://id.example.com").validate().is_err());
    assert!(config("https://id.example.com/?x=1").validate().is_err());

    let mut c = config("https://id.example.com");
    c.public_url = None;
    assert!(c.validate().is_err(), "needs a redirect URI");
    c.redirect_uri = Some("http://cms.example.com/cb".into());
    assert!(c.validate().is_err(), "remote http redirect");

    let mut c = config("https://id.example.com");
    c.scopes = vec!["email".into()];
    assert!(c.validate().is_err(), "openid scope required");
    let mut c = config("https://id.example.com");
    c.min_aal = Some(4);
    assert!(c.validate().is_err());
    let mut c = config("https://id.example.com");
    c.post_login_url = "//evil.example/".into();
    assert!(c.validate().is_err(), "no open redirect");
    c.post_login_url = "https://evil.example/".into();
    assert!(c.validate().is_err());
}
