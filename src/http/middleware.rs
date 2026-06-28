use axum::{
    extract::{Request, State},
    http::header::AUTHORIZATION,
    middleware::Next,
    response::{IntoResponse, Response},
};
use crate::schema::SchemaContext;

use crate::http::auth::decode_jwt;

#[derive(Clone)]
pub struct AuthMiddlewareState {
    pub jwt_secret: String,
    pub ctx_template: SchemaContext,
}

/// JWT auth middleware — attaches SchemaContext with user info to request extensions.
/// Public routes can still proceed (ctx has no user_id).
pub async fn jwt_auth_middleware(
    State(state): State<AuthMiddlewareState>,
    mut request: Request,
    next: Next,
) -> Response {
    let mut ctx = state.ctx_template.clone();

    if let Some(auth_header) = request.headers().get(AUTHORIZATION) {
        if let Ok(header_str) = auth_header.to_str() {
            if let Some(token) = header_str.strip_prefix("Bearer ") {
                if let Ok(claims) = decode_jwt(token, &state.jwt_secret) {
                    ctx = ctx.with_user(claims.sub, claims.roles);
                }
            }
        }
    }

    request.extensions_mut().insert(ctx);
    next.run(request).await
}
