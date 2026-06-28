use argon2::{Argon2, PasswordHash, PasswordVerifier};
use axum::{
    extract::{Json, State},
    http::StatusCode,
    response::IntoResponse,
};
use chrono::Utc;
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

use crate::core::{CmsError, ContentStore};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtClaims {
    pub sub: String,
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
    let invalid = || {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "invalid credentials"})),
        )
            .into_response()
    };

    let user = match state.store.get_user_by_email(&req.email).await {
        Ok(u) => u,
        Err(CmsError::NotFound(_)) => return invalid(),
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

    let parsed = match PasswordHash::new(&hash) {
        Ok(p) => p,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    if Argon2::default()
        .verify_password(req.password.as_bytes(), &parsed)
        .is_err()
    {
        return invalid();
    }

    let exp = Utc::now().timestamp() + 86_400;
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
