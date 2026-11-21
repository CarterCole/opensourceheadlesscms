use argon2::{
    password_hash::{rand_core::OsRng, SaltString},
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
};
use axum::{
    extract::{Extension, Json, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use chrono::Utc;
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

use crate::core::{CmsError, ContentStore};
use crate::http::ratelimit::{retry_response, ClientIp, LoginThrottle};
use crate::schema::SchemaContext;
use std::sync::{Arc, OnceLock};

/// Name of the HttpOnly session cookie used by the browser admin UI.
pub const SESSION_COOKIE: &str = "cms_session";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtClaims {
    pub sub: String,
    pub email: String,
    /// Informational only: the server always re-reads roles from the database.
    pub roles: Vec<String>,
    pub exp: i64,
    /// Session epoch the token was issued under; the token dies when the user's epoch moves.
    #[serde(default)]
    pub ep: i64,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: i64,
    pub user_id: String,
    pub roles: Vec<String>,
}

#[derive(Clone)]
pub struct AuthState {
    pub store: Arc<dyn ContentStore>,
    pub jwt_secret: String,
    pub jwt_ttl_seconds: i64,
    pub cookie_secure: bool,
    pub throttle: Arc<LoginThrottle>,
}

/// A valid Argon2 hash of a throwaway password. Verified against when the account
/// doesn't exist so unknown-email and wrong-password logins take the same time
/// (otherwise response latency reveals which emails are registered).
fn dummy_hash() -> &'static str {
    static HASH: OnceLock<String> = OnceLock::new();
    HASH.get_or_init(|| {
        let salt = SaltString::generate(&mut OsRng);
        Argon2::default()
            .hash_password(b"timing-equalisation-placeholder", &salt)
            .map(|h| h.to_string())
            .unwrap_or_default()
    })
}

/// Argon2 is deliberately slow; run it off the async executor threads.
async fn verify_password(hash: String, password: String) -> bool {
    tokio::task::spawn_blocking(move || match PasswordHash::new(&hash) {
        Ok(parsed) => Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok(),
        Err(_) => false,
    })
    .await
    .unwrap_or(false)
}

pub fn encode_jwt(claims: &JwtClaims, secret: &str) -> Result<String, CmsError> {
    encode(
        &Header::default(),
        claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|e| CmsError::Internal(format!("jwt encode: {e}")))
}

pub fn decode_jwt(token: &str, secret: &str) -> Result<JwtClaims, CmsError> {
    let mut validation = Validation::new(Algorithm::HS256);
    // jsonwebtoken defaults to 60s of leeway, which keeps expired tokens alive.
    validation.leeway = 0;
    validation.set_required_spec_claims(&["exp", "sub"]);
    decode::<JwtClaims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .map(|data| data.claims)
    .map_err(|_| CmsError::Unauthorized)
}

/// Validate a session token against the database and return `(user_id, roles)`.
///
/// The signature and expiry are checked first, then the user must still exist and the
/// token's epoch must match the user's current epoch (so logout, deletion and revocation
/// take effect immediately). Roles come from the database, not from the token.
pub async fn authenticate_token(
    store: &dyn ContentStore,
    secret: &str,
    token: &str,
) -> Option<(String, Vec<String>)> {
    let claims = decode_jwt(token, secret).ok()?;
    let (roles, epoch) = store.get_auth_state(&claims.sub).await.ok()?;
    if epoch != claims.ep {
        return None;
    }
    Some((claims.sub, roles))
}

pub(crate) fn session_cookie(token: &str, max_age: i64, secure: bool) -> HeaderValue {
    let mut value =
        format!("{SESSION_COOKIE}={token}; HttpOnly; SameSite=Strict; Path=/; Max-Age={max_age}");
    if secure {
        value.push_str("; Secure");
    }
    HeaderValue::from_str(&value).unwrap_or_else(|_| HeaderValue::from_static(""))
}

#[utoipa::path(
    post,
    path = "/api/auth/login",
    tag = "Auth",
    request_body(content = crate::http::openapi::LoginRequestSchema, description = "Email and password", content_type = "application/json"),
    responses(
        (status = 200, description = "JWT bearer token (also set as an HttpOnly session cookie)", body = crate::http::openapi::LoginResponseSchema),
        (status = 401, description = "Invalid credentials", body = crate::http::openapi::ErrorResponse),
        (status = 429, description = "Too many failed attempts"),
        (status = 500, description = "Internal server error"),
    )
)]
pub async fn login_handler(
    State(state): State<AuthState>,
    Extension(ip): Extension<ClientIp>,
    Json(req): Json<LoginRequest>,
) -> Response {
    let invalid = || {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "invalid credentials"})),
        )
            .into_response()
    };

    if let Err(retry_after) = state.throttle.check(&ip.0, &req.email) {
        return retry_response(retry_after);
    }

    let user = match state.store.get_user_by_email(&req.email).await {
        Ok(u) => u,
        Err(CmsError::NotFound(_)) => {
            let _ = verify_password(dummy_hash().to_string(), req.password).await;
            state.throttle.record_failure(&ip.0, &req.email);
            return invalid();
        }
        Err(e) => {
            tracing::error!("login db error: {e}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let hash = match state.store.get_password_hash(&req.email).await {
        Ok(h) => h,
        Err(e) => {
            tracing::error!("get_password_hash error: {e}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    if PasswordHash::new(&hash).is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    if !verify_password(hash, req.password.clone()).await {
        state.throttle.record_failure(&ip.0, &req.email);
        return invalid();
    }
    state.throttle.record_success(&ip.0, &req.email);

    let epoch = match state.store.get_auth_state(&user.id).await {
        Ok((_, epoch)) => epoch,
        Err(e) => {
            tracing::error!("get_auth_state error: {e}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let ttl = state.jwt_ttl_seconds;
    let claims = JwtClaims {
        sub: user.id.clone(),
        email: user.email.clone(),
        roles: user.roles.clone(),
        exp: Utc::now().timestamp() + ttl,
        ep: epoch,
    };

    match encode_jwt(&claims, &state.jwt_secret) {
        Ok(token) => {
            let cookie = session_cookie(&token, ttl, state.cookie_secure);
            let mut resp = (
                StatusCode::OK,
                Json(LoginResponse {
                    access_token: token,
                    token_type: "Bearer".to_string(),
                    expires_in: ttl,
                    user_id: user.id,
                    roles: user.roles,
                }),
            )
                .into_response();
            resp.headers_mut().append(header::SET_COOKIE, cookie);
            resp.headers_mut()
                .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
            resp
        }
        Err(e) => {
            tracing::error!("jwt error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// POST /api/auth/logout — revokes every session token the user holds and clears the cookie.
pub async fn logout_handler(
    State(state): State<AuthState>,
    Extension(ctx): Extension<SchemaContext>,
) -> Response {
    if let Some(user_id) = ctx.user_id.as_deref() {
        if let Err(e) = state.store.bump_session_epoch(user_id).await {
            tracing::error!("logout: bump_session_epoch: {e}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    }
    let mut resp = StatusCode::NO_CONTENT.into_response();
    resp.headers_mut().append(
        header::SET_COOKIE,
        session_cookie("", 0, state.cookie_secure),
    );
    resp
}

/// GET /api/auth/me — who the current session belongs to (the browser can't read the
/// HttpOnly cookie, so the admin UI asks the server).
pub async fn me_handler(
    State(state): State<AuthState>,
    Extension(ctx): Extension<SchemaContext>,
) -> Response {
    let Some(user_id) = ctx.user_id.as_deref() else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    match state.store.get_user(user_id).await {
        Ok(user) => {
            let mut resp = Json(serde_json::json!({
                "user_id": user.id,
                "email": user.email,
                "display_name": user.display_name,
                "roles": ctx.user_roles,
            }))
            .into_response();
            resp.headers_mut()
                .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
            resp
        }
        Err(_) => StatusCode::UNAUTHORIZED.into_response(),
    }
}
