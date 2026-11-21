//! OpenID Connect sign-in for the admin UI ("Sign in with Kith").
//!
//! The server is a relying party: `GET /api/auth/oidc/login` sends the browser to the
//! provider (authorization code flow with PKCE), `GET /api/auth/oidc/callback` redeems the
//! code, checks the ID token, finds (or creates) the CMS user by verified email and then
//! issues the same session cookie as the password login.
//!
//! Browser binding: the `state`, `nonce` and PKCE verifier live in a short-lived signed
//! cookie, so the callback only completes in the browser that started the flow (login CSRF)
//! and the server keeps no pending-login state.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::{
    extract::{Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{Html, IntoResponse, Redirect, Response},
    routing::get,
    Json, Router,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::Utc;
use jsonwebtoken::{
    decode, decode_header, encode,
    jwk::{AlgorithmParameters, JwkSet},
    Algorithm, DecodingKey, EncodingKey, Header, Validation,
};
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::{Mutex, RwLock};

use crate::config::OidcConfig;
use crate::core::{CmsError, ContentStore};
use crate::http::auth::{encode_jwt, session_cookie, JwtClaims};

const STATE_COOKIE: &str = "cms_oidc";
const STATE_COOKIE_PATH: &str = "/api/auth/oidc";
const STATE_TTL_SECONDS: i64 = 600;
const DISCOVERY_TTL: Duration = Duration::from_secs(3600);
const JWKS_TTL: Duration = Duration::from_secs(3600);
/// Unknown `kid`s trigger a JWKS refetch (key rotation); never more often than this.
const JWKS_MIN_REFETCH: Duration = Duration::from_secs(60);
const HTTP_TIMEOUT: Duration = Duration::from_secs(10);

// ------------------------------------------------------------------------------------
// Errors
// ------------------------------------------------------------------------------------

#[derive(Debug)]
pub enum OidcError {
    /// The provider could not be reached or answered nonsense. Details go to the log only.
    Unavailable(String),
    /// The sign-in was refused. The message is safe to show.
    Rejected(&'static str),
}

impl From<reqwest::Error> for OidcError {
    fn from(e: reqwest::Error) -> Self {
        // reqwest errors can embed the request URL (and so the authorization code).
        OidcError::Unavailable(e.without_url().to_string())
    }
}

// ------------------------------------------------------------------------------------
// Provider metadata and ID token
// ------------------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
struct Discovery {
    issuer: String,
    authorization_endpoint: String,
    token_endpoint: String,
    jwks_uri: String,
    #[serde(default)]
    code_challenge_methods_supported: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    id_token: String,
}

/// The claims we use. Kith puts `email`, `name` and `roles`/`groups` in the ID token
/// according to the scopes the user agreed to.
#[derive(Debug, Deserialize)]
struct IdClaims {
    sub: String,
    #[serde(default)]
    nonce: Option<String>,
    #[serde(default)]
    azp: Option<String>,
    #[serde(default)]
    acr: Option<String>,
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    email_verified: Option<bool>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    preferred_username: Option<String>,
    #[serde(default)]
    roles: Vec<String>,
    #[serde(default)]
    groups: Vec<String>,
}

#[derive(Default)]
struct JwksCache {
    keys: Option<JwkSet>,
    fetched: Option<Instant>,
}

pub struct OidcClient {
    cfg: OidcConfig,
    redirect_uri: String,
    http: reqwest::Client,
    discovery: RwLock<Option<(Instant, Arc<Discovery>)>>,
    jwks: Mutex<JwksCache>,
}

impl OidcClient {
    pub fn new(cfg: OidcConfig) -> Result<Self, String> {
        cfg.validate()?;
        let redirect_uri = cfg.redirect_uri().ok_or("oidc: no redirect URI")?;
        let http = reqwest::Client::builder()
            .timeout(HTTP_TIMEOUT)
            // A provider that redirects its token or JWKS endpoint is misconfigured, and
            // following it could send the client secret somewhere else.
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(concat!("opensourceheadlesscms/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| format!("oidc http client: {e}"))?;
        Ok(Self {
            cfg,
            redirect_uri,
            http,
            discovery: RwLock::new(None),
            jwks: Mutex::new(JwksCache::default()),
        })
    }

    fn issuer(&self) -> &str {
        self.cfg.issuer.trim_end_matches('/')
    }

    async fn get_json<T: serde::de::DeserializeOwned>(&self, url: &str) -> Result<T, OidcError> {
        let resp = self.http.get(url).send().await?;
        if !resp.status().is_success() {
            return Err(OidcError::Unavailable(format!(
                "GET {url}: HTTP {}",
                resp.status()
            )));
        }
        Ok(resp.json().await?)
    }

    async fn discovery(&self) -> Result<Arc<Discovery>, OidcError> {
        if let Some((at, d)) = self.discovery.read().await.as_ref() {
            if at.elapsed() < DISCOVERY_TTL {
                return Ok(d.clone());
            }
        }
        let url = format!("{}/.well-known/openid-configuration", self.issuer());
        let d: Discovery = self.get_json(&url).await?;
        // OIDC Discovery §4.3: the document must name the issuer it was fetched for.
        if d.issuer.trim_end_matches('/') != self.issuer() {
            return Err(OidcError::Unavailable(format!(
                "discovery issuer {:?} does not match the configured issuer",
                d.issuer
            )));
        }
        if !d.code_challenge_methods_supported.is_empty()
            && !d
                .code_challenge_methods_supported
                .iter()
                .any(|m| m == "S256")
        {
            return Err(OidcError::Unavailable(
                "provider does not support PKCE S256".into(),
            ));
        }
        for endpoint in [&d.authorization_endpoint, &d.token_endpoint, &d.jwks_uri] {
            let ok = url::Url::parse(endpoint)
                .map(|u| u.scheme() == "https" || u.scheme() == "http")
                .unwrap_or(false);
            if !ok {
                return Err(OidcError::Unavailable(format!(
                    "bad endpoint in discovery: {endpoint}"
                )));
            }
        }
        let d = Arc::new(d);
        *self.discovery.write().await = Some((Instant::now(), d.clone()));
        Ok(d)
    }

    /// The URL to send the browser to.
    async fn authorize_url(
        &self,
        state: &str,
        nonce: &str,
        challenge: &str,
    ) -> Result<String, OidcError> {
        let d = self.discovery().await?;
        let mut url = url::Url::parse(&d.authorization_endpoint)
            .map_err(|e| OidcError::Unavailable(format!("authorization_endpoint: {e}")))?;
        {
            let mut q = url.query_pairs_mut();
            q.append_pair("response_type", "code");
            q.append_pair("client_id", &self.cfg.client_id);
            q.append_pair("redirect_uri", &self.redirect_uri);
            q.append_pair("scope", &self.cfg.scopes.join(" "));
            q.append_pair("state", state);
            q.append_pair("nonce", nonce);
            q.append_pair("code_challenge", challenge);
            q.append_pair("code_challenge_method", "S256");
            if let Some(aal) = self.cfg.min_aal {
                q.append_pair("acr_values", &format!("urn:kith:acr:aal{aal}"));
            }
        }
        Ok(url.into())
    }

    /// Redeem the authorization code; returns the raw ID token.
    async fn exchange_code(&self, code: &str, verifier: &str) -> Result<String, OidcError> {
        let d = self.discovery().await?;
        let mut form = vec![
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", self.redirect_uri.as_str()),
            ("code_verifier", verifier),
        ];
        let mut req = self.http.post(&d.token_endpoint);
        match self.cfg.client_secret.as_deref().filter(|s| !s.is_empty()) {
            Some(secret) => req = req.basic_auth(&self.cfg.client_id, Some(secret)),
            None => form.push(("client_id", self.cfg.client_id.as_str())),
        }
        let resp = req.form(&form).send().await?;
        if !resp.status().is_success() {
            return Err(OidcError::Unavailable(format!(
                "token endpoint: HTTP {}",
                resp.status()
            )));
        }
        let tokens: TokenResponse = resp.json().await?;
        Ok(tokens.id_token)
    }

    /// An RSA signing key by `kid`. An unknown `kid` refetches the JWKS once (rotation).
    async fn decoding_key(&self, kid: Option<&str>) -> Result<DecodingKey, OidcError> {
        let d = self.discovery().await?;
        let mut cache = self.jwks.lock().await;

        let find = |set: &JwkSet| -> Option<DecodingKey> {
            let mut rsa = set
                .keys
                .iter()
                .filter(|k| matches!(k.algorithm, AlgorithmParameters::RSA(_)));
            let jwk = match kid {
                Some(kid) => rsa.find(|k| k.common.key_id.as_deref() == Some(kid)),
                // No kid: only unambiguous if the provider has exactly one RSA key.
                None => {
                    let first = rsa.next();
                    if rsa.next().is_some() {
                        None
                    } else {
                        first
                    }
                }
            }?;
            DecodingKey::from_jwk(jwk).ok()
        };

        let fresh = cache.fetched.is_some_and(|t| t.elapsed() < JWKS_TTL);
        if fresh {
            if let Some(key) = cache.keys.as_ref().and_then(find) {
                return Ok(key);
            }
        }
        let may_refetch = cache
            .fetched
            .map(|t| t.elapsed() >= JWKS_MIN_REFETCH)
            .unwrap_or(true);
        if !may_refetch {
            return Err(OidcError::Rejected("unknown signing key"));
        }
        let set: JwkSet = self.get_json(&d.jwks_uri).await?;
        cache.fetched = Some(Instant::now());
        let key = find(&set);
        cache.keys = Some(set);
        key.ok_or(OidcError::Rejected("unknown signing key"))
    }

    async fn verify_id_token(&self, token: &str, nonce: &str) -> Result<IdClaims, OidcError> {
        let header = decode_header(token).map_err(|_| OidcError::Rejected("bad ID token"))?;
        // Only RS256 (what OIDC Core requires of every provider): never `none`, never HMAC
        // keyed with something public.
        if header.alg != Algorithm::RS256 {
            return Err(OidcError::Rejected("unsupported ID token algorithm"));
        }
        let key = self.decoding_key(header.kid.as_deref()).await?;

        let mut v = Validation::new(Algorithm::RS256);
        v.set_issuer(&[self.issuer()]);
        v.set_audience(&[self.cfg.client_id.as_str()]);
        v.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);
        v.leeway = 30;
        let claims = decode::<IdClaims>(token, &key, &v)
            .map_err(|e| {
                tracing::warn!("oidc: ID token rejected: {e}");
                OidcError::Rejected("ID token failed validation")
            })?
            .claims;

        if claims.nonce.as_deref() != Some(nonce) {
            return Err(OidcError::Rejected("ID token nonce mismatch"));
        }
        // OIDC Core §3.1.3.7: with several audiences, azp must be us.
        if let Some(azp) = claims.azp.as_deref() {
            if azp != self.cfg.client_id {
                return Err(OidcError::Rejected("ID token was issued to another client"));
            }
        }
        if let Some(min) = self.cfg.min_aal {
            let aal = claims
                .acr
                .as_deref()
                .and_then(|a| a.rsplit_once("aal"))
                .and_then(|(_, n)| n.parse::<u8>().ok());
            if aal.is_none_or(|a| a < min) {
                return Err(OidcError::Rejected(
                    "sign-in did not meet the required assurance level",
                ));
            }
        }
        Ok(claims)
    }
}

// ------------------------------------------------------------------------------------
// Role mapping
// ------------------------------------------------------------------------------------

/// The CMS roles a provider's `roles` and `groups` map to through `role_map`, sorted.
fn map_roles(cfg: &OidcConfig, claims: &IdClaims) -> Vec<String> {
    claims
        .roles
        .iter()
        .chain(claims.groups.iter())
        .filter_map(|name| cfg.role_map.get(name))
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

// ------------------------------------------------------------------------------------
// Browser-binding cookie
// ------------------------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
struct PendingLogin {
    /// Never accepted as a session token: the shape differs and the key is derived.
    typ: String,
    state: String,
    nonce: String,
    verifier: String,
    exp: i64,
}

fn state_key(jwt_secret: &str) -> String {
    format!("{jwt_secret}|oidc-state")
}

fn random_token() -> String {
    let mut b = [0u8; 32];
    OsRng.fill_bytes(&mut b);
    URL_SAFE_NO_PAD.encode(b)
}

fn state_cookie(value: &str, max_age: i64, secure: bool) -> HeaderValue {
    // Lax, not Strict: the provider sends the browser back with a cross-site navigation,
    // and the cookie has to come along with it.
    let mut c = format!(
        "{STATE_COOKIE}={value}; HttpOnly; SameSite=Lax; Path={STATE_COOKIE_PATH}; Max-Age={max_age}"
    );
    if secure {
        c.push_str("; Secure");
    }
    HeaderValue::from_str(&c).unwrap_or_else(|_| HeaderValue::from_static(""))
}

fn read_cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(n, _)| *n == name)
        .map(|(_, v)| v.to_string())
        .filter(|v| !v.is_empty())
}

// ------------------------------------------------------------------------------------
// Handlers
// ------------------------------------------------------------------------------------

#[derive(Clone)]
pub struct OidcState {
    /// `None` when `[oidc]` is not configured: the routes then answer as if absent.
    pub client: Option<Arc<OidcClient>>,
    pub store: Arc<dyn ContentStore>,
    pub jwt_secret: String,
    pub jwt_ttl_seconds: i64,
    pub cookie_secure: bool,
}

pub fn router(state: OidcState) -> Router {
    Router::new()
        .route("/api/auth/oidc/config", get(config_handler))
        .route("/api/auth/oidc/login", get(login_handler))
        .route("/api/auth/oidc/callback", get(callback_handler))
        .with_state(state)
}

/// GET /api/auth/oidc/config — lets the login page know whether to show the button.
async fn config_handler(State(state): State<OidcState>) -> Response {
    let body = match &state.client {
        Some(c) => serde_json::json!({"enabled": true, "name": c.cfg.name}),
        None => serde_json::json!({"enabled": false}),
    };
    let mut resp = Json(body).into_response();
    resp.headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    resp
}

/// A small static page for a failed sign-in. Only fixed text is ever shown: nothing from
/// the query string or the provider is echoed.
fn error_page(state: &OidcState, status: StatusCode, message: &'static str) -> Response {
    let back = state
        .client
        .as_ref()
        .map(|c| c.cfg.post_login_url.as_str())
        .unwrap_or("/admin/");
    let html = format!(
        "<!doctype html><meta charset=utf-8><meta name=viewport content=\"width=device-width\">\
         <title>Sign-in failed</title>\
         <body style=\"font:16px system-ui;max-width:28rem;margin:4rem auto;padding:0 1rem\">\
         <h1 style=\"font-size:1.25rem\">Sign-in failed</h1><p>{message}</p>\
         <p><a href=\"{back}\">Back to sign in</a></p></body>"
    );
    let mut resp = (status, Html(html)).into_response();
    let h = resp.headers_mut();
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    h.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("default-src 'none'; style-src 'unsafe-inline'"),
    );
    h.append(header::SET_COOKIE, state_cookie("", 0, state.cookie_secure));
    resp
}

/// GET /api/auth/oidc/login — start the flow.
async fn login_handler(State(state): State<OidcState>) -> Response {
    let Some(client) = state.client.clone() else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let (csrf, nonce, verifier) = (random_token(), random_token(), random_token());
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));

    let url = match client.authorize_url(&csrf, &nonce, &challenge).await {
        Ok(u) => u,
        Err(e) => {
            tracing::error!("oidc login: {e:?}");
            return error_page(
                &state,
                StatusCode::BAD_GATEWAY,
                "The sign-in provider is not reachable. Try again later.",
            );
        }
    };

    let pending = PendingLogin {
        typ: "oidc-state".into(),
        state: csrf,
        nonce,
        verifier,
        exp: Utc::now().timestamp() + STATE_TTL_SECONDS,
    };
    let Ok(cookie_value) = encode(
        &Header::default(),
        &pending,
        &EncodingKey::from_secret(state_key(&state.jwt_secret).as_bytes()),
    ) else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };

    let mut resp = Redirect::to(&url).into_response();
    let h = resp.headers_mut();
    h.append(
        header::SET_COOKIE,
        state_cookie(&cookie_value, STATE_TTL_SECONDS, state.cookie_secure),
    );
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    resp
}

#[derive(Deserialize)]
struct CallbackParams {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
    /// RFC 9207 issuer identification (Kith sends it).
    iss: Option<String>,
}

/// GET /api/auth/oidc/callback — finish the flow and start a CMS session.
async fn callback_handler(
    State(state): State<OidcState>,
    headers: HeaderMap,
    Query(params): Query<CallbackParams>,
) -> Response {
    let Some(client) = state.client.clone() else {
        return StatusCode::NOT_FOUND.into_response();
    };

    match complete_login(&state, &client, &headers, params).await {
        Ok(resp) => resp,
        Err(OidcError::Rejected(msg)) => {
            tracing::warn!("oidc callback rejected: {msg}");
            error_page(&state, StatusCode::FORBIDDEN, msg)
        }
        Err(OidcError::Unavailable(detail)) => {
            tracing::error!("oidc callback: {detail}");
            error_page(
                &state,
                StatusCode::BAD_GATEWAY,
                "The sign-in provider did not answer correctly. Try again later.",
            )
        }
    }
}

async fn complete_login(
    state: &OidcState,
    client: &OidcClient,
    headers: &HeaderMap,
    params: CallbackParams,
) -> Result<Response, OidcError> {
    // 1. Same browser as the one that started the flow, and the state matches.
    let cookie = read_cookie(headers, STATE_COOKIE)
        .ok_or(OidcError::Rejected("sign-in expired; start again"))?;
    let mut validation = Validation::new(Algorithm::HS256);
    validation.leeway = 0;
    validation.set_required_spec_claims(&["exp"]);
    let pending = decode::<PendingLogin>(
        &cookie,
        &DecodingKey::from_secret(state_key(&state.jwt_secret).as_bytes()),
        &validation,
    )
    .map(|d| d.claims)
    .map_err(|_| OidcError::Rejected("sign-in expired; start again"))?;
    if pending.typ != "oidc-state" || params.state.as_deref() != Some(pending.state.as_str()) {
        return Err(OidcError::Rejected("sign-in state mismatch; start again"));
    }

    // 2. The provider's answer.
    if params.error.is_some() {
        return Err(OidcError::Rejected("the provider declined the sign-in"));
    }
    if let Some(iss) = params.iss.as_deref() {
        if iss.trim_end_matches('/') != client.issuer() {
            return Err(OidcError::Rejected("response came from another provider"));
        }
    }
    let code = params
        .code
        .as_deref()
        .ok_or(OidcError::Rejected("no authorization code"))?;

    let id_token = client.exchange_code(code, &pending.verifier).await?;
    let claims = client.verify_id_token(&id_token, &pending.nonce).await?;

    // 3. Which CMS user?
    let email = claims
        .email
        .as_deref()
        .map(str::trim)
        .filter(|e| !e.is_empty() && claims.email_verified == Some(true))
        .ok_or(OidcError::Rejected(
            "the provider did not share a verified email address",
        ))?
        .to_string();
    let user = resolve_user(state, &client.cfg, &claims, &email).await?;

    tracing::info!(user = %user.id, sub = %claims.sub, "oidc sign-in");

    // 4. Same session as a password login.
    let (_, epoch) = state
        .store
        .get_auth_state(&user.id)
        .await
        .map_err(|e| OidcError::Unavailable(format!("get_auth_state: {e}")))?;
    let ttl = state.jwt_ttl_seconds;
    let token = encode_jwt(
        &JwtClaims {
            sub: user.id.clone(),
            email: user.email.clone(),
            roles: user.roles.clone(),
            exp: Utc::now().timestamp() + ttl,
            ep: epoch,
        },
        &state.jwt_secret,
    )
    .map_err(|e| OidcError::Unavailable(format!("{e}")))?;

    let mut resp = Redirect::to(&client.cfg.post_login_url).into_response();
    let h = resp.headers_mut();
    h.append(
        header::SET_COOKIE,
        session_cookie(&token, ttl, state.cookie_secure),
    );
    h.append(header::SET_COOKIE, state_cookie("", 0, state.cookie_secure));
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(resp)
}

/// Find the CMS user for a verified email, creating one when `auto_create` allows it, and
/// apply `role_map`.
async fn resolve_user(
    state: &OidcState,
    cfg: &OidcConfig,
    claims: &IdClaims,
    email: &str,
) -> Result<crate::core::User, OidcError> {
    let internal = |what: &str, e: CmsError| OidcError::Unavailable(format!("{what}: {e}"));

    let mut found = match state.store.get_user_by_email(email).await {
        Ok(u) => Some(u),
        Err(CmsError::NotFound(_)) => None,
        Err(e) => return Err(internal("get_user_by_email", e)),
    };
    // Providers lower-case addresses; an account may have been created with capitals.
    if found.is_none() {
        found = match state.store.get_user_by_email_ci(email).await {
            Ok(u) => Some(u),
            Err(CmsError::NotFound(_)) => None,
            // Two accounts that differ only by case: do not guess which one is this person.
            Err(CmsError::Validation(_)) => {
                return Err(OidcError::Rejected(
                    "more than one account matches this email; ask an administrator",
                ))
            }
            Err(e) => return Err(internal("get_user_by_email_ci", e)),
        };
    }

    // With a role_map the provider decides the roles; with none mapped, the defaults apply.
    let mapped = if cfg.role_map.is_empty() {
        None
    } else {
        let m = map_roles(cfg, claims);
        Some(if m.is_empty() {
            cfg.default_roles.clone()
        } else {
            m
        })
    };

    match found {
        Some(user) => match mapped {
            Some(roles) if roles != user.roles => state
                .store
                .update_user_roles(&user.id, roles)
                .await
                .map_err(|e| internal("update_user_roles", e)),
            _ => Ok(user),
        },
        None if cfg.auto_create => {
            let display = claims
                .name
                .as_deref()
                .or(claims.preferred_username.as_deref())
                .map(str::trim)
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| email.split('@').next().unwrap_or(email))
                .to_string();
            let roles = mapped.unwrap_or_else(|| cfg.default_roles.clone());
            let hash = unusable_password_hash().await;
            state
                .store
                .create_user(email, &display, &hash, roles)
                .await
                .map_err(|e| internal("create_user", e))
        }
        None => Err(OidcError::Rejected(
            "there is no account for this identity; ask an administrator to add you",
        )),
    }
}

/// A valid Argon2 hash of a random value nobody sees, so an account made by SSO cannot
/// be entered with a password until someone sets one.
async fn unusable_password_hash() -> String {
    use argon2::{
        password_hash::{rand_core::OsRng as ArgonOsRng, SaltString},
        Argon2, PasswordHasher,
    };
    let secret = random_token();
    tokio::task::spawn_blocking(move || {
        let salt = SaltString::generate(&mut ArgonOsRng);
        Argon2::default()
            .hash_password(secret.as_bytes(), &salt)
            .map(|h| h.to_string())
            .unwrap_or_default()
    })
    .await
    .unwrap_or_default()
}
