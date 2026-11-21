use axum::{response::IntoResponse, Json};
use serde::Serialize;
use utoipa::{
    openapi::security::{Http, HttpAuthScheme, SecurityScheme},
    Modify, OpenApi, ToSchema,
};

// ── Response schema types ──────────────────────────────────────────────────

#[derive(Serialize, ToSchema)]
pub struct ContentItemResponse {
    pub id: String,
    pub content_type: String,
    pub status: String,
    /// Arbitrary key/value field data (schema depends on the content type definition)
    pub fields: serde_json::Value,
    pub version: u32,
    pub created_at: String,
    pub updated_at: String,
    pub created_by: String,
    pub scheduled_at: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct ContentListResponse {
    pub items: Vec<ContentItemResponse>,
    pub total: u64,
    pub page: u32,
    pub per_page: u32,
}

#[derive(Serialize, ToSchema)]
pub struct SearchResponse {
    pub items: Vec<ContentItemResponse>,
    pub total: u64,
}

#[derive(Serialize, ToSchema)]
pub struct HealthResponse {
    pub status: String,
}

#[derive(Serialize, ToSchema)]
pub struct ErrorResponse {
    pub error: String,
}

/// Request body for `POST /api/auth/login`
#[derive(Serialize, ToSchema)]
pub struct LoginRequestSchema {
    pub email: String,
    pub password: String,
}

#[derive(Serialize, ToSchema)]
pub struct LoginResponseSchema {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: i64,
    pub user_id: String,
    pub roles: Vec<String>,
}

// ── Security modifier ──────────────────────────────────────────────────────

struct BearerAuth;

impl Modify for BearerAuth {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "bearer_auth",
                SecurityScheme::Http(Http::new(HttpAuthScheme::Bearer)),
            );
        }
    }
}

// ── OpenAPI document ───────────────────────────────────────────────────────

#[derive(OpenApi)]
#[openapi(
    info(
        title = "OpenSource Headless CMS",
        version = "1.0.0",
        description = "REST API for the OpenSource Headless CMS. All write endpoints require a Bearer token obtained from `POST /api/auth/login`. Public read endpoints (`GET /api/v1/*`) are unauthenticated and CDN-cacheable.",
        license(name = "AGPL-3.0", url = "https://www.gnu.org/licenses/agpl-3.0.html"),
        contact(name = "opensourceheadlesscms", url = "https://github.com/CarterCole/opensourceheadlesscms"),
    ),
    servers(
        (url = "/", description = "This server"),
    ),
    paths(
        crate::http::rest::health_handler,
        crate::http::rest::list_handler,
        crate::http::rest::get_by_slug_handler,
        crate::http::rest::search_handler,
        crate::http::auth::login_handler,
    ),
    components(
        schemas(
            ContentItemResponse,
            ContentListResponse,
            SearchResponse,
            HealthResponse,
            ErrorResponse,
            LoginRequestSchema,
            LoginResponseSchema,
        )
    ),
    tags(
        (name = "Content", description = "Public CDN-cacheable content endpoints (no auth required)"),
        (name = "Auth",    description = "Authentication — obtain a Bearer token"),
        (name = "System",  description = "Operational endpoints"),
    ),
    modifiers(&BearerAuth),
)]
pub struct ApiDoc;

// ── Route handlers ─────────────────────────────────────────────────────────

pub async fn openapi_json_handler() -> impl IntoResponse {
    Json(ApiDoc::openapi())
}
