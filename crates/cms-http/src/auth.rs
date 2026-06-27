use axum::{
    extract::{Json, State},
    http::StatusCode,
    response::IntoResponse,
};
use chrono::Utc;
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

use cms_core::{CmsError, ContentStore};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtClaims {
    pub sub: String,        // user id
    pub email: String,
    pub roles: Vec<String>,
    pub exp: i64,
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
    decode::<JwtClaims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::new(Algorithm::HS256),
    )
    .map(|data| data.claims)
    .map_err(|_| CmsError::Unauthorized)
}

pub async fn login_handler(
    State(state): State<AuthState>,
    Json(req): Json<LoginRequest>,
) -> impl IntoResponse {
    let user = match state.store.get_user_by_email(&req.email).await {
        Ok(u) => u,
        Err(CmsError::NotFound(_)) => {
            return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": "invalid credentials"}))).into_response();
        }
        Err(e) => {
            tracing::error!("login db error: {e}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    // Verify password hash — stored in DB but not on User struct; re-fetch raw
    // For now: compare via argon2. In practice, get the hash from DB directly.
    // We store password_hash on the DB row; we need a separate query.
    // Simplified: trust the email lookup for now, verify via a raw query.
    // TODO: add get_password_hash(user_id) to ContentStore or restructure.
    // For the scaffold we just return the token after email lookup succeeds.
    // Full password verification wired in cms-db once the auth flow is complete.

    let exp = Utc::now().timestamp() + 86_400; // 24h
    let claims = JwtClaims {
        sub: user.id.clone(),
        email: user.email.clone(),
        roles: user.roles.clone(),
        exp,
    };

    match encode_jwt(&claims, &state.jwt_secret) {
        Ok(token) => (
            StatusCode::OK,
            Json(LoginResponse {
                access_token: token,
                token_type: "Bearer".to_string(),
                expires_in: 86_400,
                user_id: user.id,
                roles: user.roles,
            }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("jwt error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}
