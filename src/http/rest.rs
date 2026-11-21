use crate::core::{
    Action, AllowedFields, CacheLayer, CachedValue, CmsError, ContentItem, ContentStore, ListQuery,
    Pagination,
};
use crate::schema::SchemaContext;
use axum::{
    extract::{Extension, Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Json, Response},
};
use serde::Deserialize;
use std::sync::Arc;
use std::time::Duration;

/// Callers who are signed in *and* may read this content type can see non-published
/// items; everyone else (anonymous, or a role without read access) only ever sees
/// `published` content.
fn may_see_unpublished(ctx: &SchemaContext, content_type: &str) -> bool {
    ctx.is_authenticated() && ctx.can(Action::Read, content_type)
}

/// Fields that must be stripped from API responses for this caller.
///
/// Signed-in readers get their own role's deny-list. Anonymous callers have no role,
/// so they get the union of every role's read restrictions for the type: a field that
/// any role is barred from reading is never served publicly.
fn denied_fields(ctx: &SchemaContext, content_type: &str) -> Vec<String> {
    if may_see_unpublished(ctx, content_type) {
        return match ctx
            .rbac
            .allowed_fields(&ctx.user_roles, content_type, Action::Read)
        {
            AllowedFields::Except(d) => d,
            _ => vec![],
        };
    }
    // No roles: the engine's view already hides fields restricted by content-type
    // `access:` rules; then add anything a role's own deny-list restricts.
    let mut denied = match ctx.rbac.allowed_fields(&[], content_type, Action::Read) {
        AllowedFields::Except(d) => d,
        _ => vec![],
    };
    for role in ctx.config().roles.values() {
        if let Some(perms) = role.permissions_for(content_type) {
            for (name, fp) in &perms.fields {
                if !fp.read && !denied.contains(name) {
                    denied.push(name.clone());
                }
            }
        }
    }
    denied
}

fn strip_denied(item: &mut ContentItem, denied: &[String]) {
    item.fields.retain(|k, _| !denied.contains(k));
}

/// Responses that depend on the caller's identity must never land in a shared cache.
fn cache_control(privileged: bool) -> &'static str {
    if privileged {
        "private, no-store"
    } else {
        "public, max-age=60"
    }
}

fn is_known_type(ctx: &SchemaContext, content_type: &str) -> bool {
    ctx.config().content_type(content_type).is_some()
}

#[derive(Clone)]
pub struct RestState {
    pub store: Arc<dyn ContentStore>,
    pub cache: Arc<dyn CacheLayer>,
}

#[derive(Deserialize)]
pub struct ListParams {
    pub status: Option<String>,
    pub page: Option<u32>,
    pub per_page: Option<u32>,
}

/// GET /api/v1/:type — list content (public, CDN-cacheable)
#[utoipa::path(
    get,
    path = "/api/v1/{type}",
    tag = "Content",
    params(
        ("type" = String, Path, description = "Content type name (e.g. `posts`, `pages`)"),
        ("status" = Option<String>, Query, description = "Filter by status (default: `published`)"),
        ("page" = Option<u32>, Query, description = "Page number, 1-indexed (default: 1)"),
        ("per_page" = Option<u32>, Query, description = "Items per page, max 100 (default: 20)"),
    ),
    responses(
        (status = 200, description = "Paginated list of content items", body = crate::http::openapi::ContentListResponse),
        (status = 500, description = "Internal server error", body = crate::http::openapi::ErrorResponse),
    )
)]
pub async fn list_handler(
    State(state): State<RestState>,
    Extension(ctx): Extension<SchemaContext>,
    Path(content_type): Path<String>,
    Query(params): Query<ListParams>,
) -> Response {
    if !is_known_type(&ctx, &content_type) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let page = params.page.unwrap_or(1).max(1);
    let per_page = params.per_page.unwrap_or(20).clamp(1, 100);
    // Only privileged callers may ask for a status other than `published`.
    let status = if may_see_unpublished(&ctx, &content_type) {
        params.status.unwrap_or_else(|| "published".to_string())
    } else {
        "published".to_string()
    };
    let privileged = may_see_unpublished(&ctx, &content_type);
    let cache_key = format!(
        "{content_type}:list:{page}:{per_page}:{status}:{}",
        if privileged { "priv" } else { "pub" }
    );

    if let Some(cached) = state.cache.get(&cache_key).await {
        if let Ok(json) = cached.to_json::<serde_json::Value>() {
            return (
                StatusCode::OK,
                [(header::CACHE_CONTROL, cache_control(privileged))],
                Json(json),
            )
                .into_response();
        }
    }

    let query = ListQuery {
        status: Some(status),
        pagination: Some(Pagination { page, per_page }),
        ..Default::default()
    };

    match state.store.list(&content_type, query).await {
        Ok(mut result) => {
            let denied = denied_fields(&ctx, &content_type);
            for item in &mut result.items {
                strip_denied(item, &denied);
            }
            let json = serde_json::json!({
                "items": result.items,
                "total": result.total,
                "page": result.page,
                "per_page": result.per_page,
            });
            if let Ok(cached) = CachedValue::from_json(&json) {
                state
                    .cache
                    .set(&cache_key, cached, Duration::from_secs(60))
                    .await;
            }
            (
                StatusCode::OK,
                [(header::CACHE_CONTROL, cache_control(privileged))],
                Json(json),
            )
                .into_response()
        }
        Err(CmsError::NotFound(_)) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            tracing::error!("list error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// GET /api/v1/:type/:slug — single content item (public, CDN-cacheable)
#[utoipa::path(
    get,
    path = "/api/v1/{type}/{slug}",
    tag = "Content",
    params(
        ("type" = String, Path, description = "Content type name"),
        ("slug" = String, Path, description = "Content item slug"),
    ),
    responses(
        (status = 200, description = "Content item", body = crate::http::openapi::ContentItemResponse),
        (status = 404, description = "Not found"),
        (status = 500, description = "Internal server error", body = crate::http::openapi::ErrorResponse),
    )
)]
pub async fn get_by_slug_handler(
    State(state): State<RestState>,
    Extension(ctx): Extension<SchemaContext>,
    Path((content_type, slug)): Path<(String, String)>,
) -> Response {
    if !is_known_type(&ctx, &content_type) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let privileged = may_see_unpublished(&ctx, &content_type);
    let cache_key = format!(
        "{content_type}:slug:{slug}:{}",
        if privileged { "priv" } else { "pub" }
    );

    if let Some(cached) = state.cache.get(&cache_key).await {
        if let Ok(json) = cached.to_json::<serde_json::Value>() {
            return (
                StatusCode::OK,
                [(header::CACHE_CONTROL, cache_control(privileged))],
                Json(json),
            )
                .into_response();
        }
    }

    match state.store.get_by_slug(&content_type, &slug).await {
        Ok(mut item) => {
            // Drafts and other unpublished states are invisible to the public.
            if !privileged && item.status != "published" {
                return StatusCode::NOT_FOUND.into_response();
            }
            strip_denied(&mut item, &denied_fields(&ctx, &content_type));
            let json = serde_json::to_value(&item).unwrap_or_default();
            if let Ok(cached) = CachedValue::from_json(&json) {
                state
                    .cache
                    .set(&cache_key, cached, Duration::from_secs(60))
                    .await;
            }
            (
                StatusCode::OK,
                [(header::CACHE_CONTROL, cache_control(privileged))],
                Json(json),
            )
                .into_response()
        }
        Err(CmsError::NotFound(_)) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            tracing::error!("get_by_slug error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// GET /health
#[utoipa::path(
    get,
    path = "/health",
    tag = "System",
    responses(
        (status = 200, description = "Service is healthy", body = crate::http::openapi::HealthResponse),
    )
)]
pub async fn health_handler() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok" }))
}

/// GET /api/v1/search
#[derive(Deserialize)]
pub struct SearchParams {
    pub q: String,
    #[serde(rename = "type")]
    pub content_type: Option<String>,
    pub page: Option<u32>,
}

/// GET /api/admin/config — content type definitions for the admin UI sidebar
pub async fn admin_config_handler(Extension(ctx): Extension<SchemaContext>) -> Response {
    // Schema/workflow definitions are for the admin UI only.
    if !ctx.is_authenticated() {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let config = ctx.config();
    let content_types: Vec<serde_json::Value> = config
        .content_types
        .values()
        .map(|ct| {
            serde_json::json!({
                "name": ct.name,
                "label": ct.label,
                "fields": ct.fields.iter().map(|f| serde_json::json!({
                    "name": f.name,
                    "type": format!("{:?}", f.field_type),
                    "required": f.required,
                    "label": f.label,
                })).collect::<Vec<_>>(),
                "workflow": ct.workflow,
            })
        })
        .collect();

    let workflows: serde_json::Value = config
        .workflows
        .iter()
        .map(|(k, w)| (k.clone(), serde_json::json!({ "states": w.states })))
        .collect();

    Json(serde_json::json!({
        "content_types": content_types,
        "workflows": workflows,
    }))
    .into_response()
}

#[utoipa::path(
    get,
    path = "/api/v1/search",
    tag = "Content",
    params(
        ("q" = String, Query, description = "Full-text search query"),
        ("type" = Option<String>, Query, description = "Restrict search to a content type"),
        ("page" = Option<u32>, Query, description = "Page number (default: 1)"),
    ),
    responses(
        (status = 200, description = "Search results", body = crate::http::openapi::SearchResponse),
        (status = 500, description = "Internal server error", body = crate::http::openapi::ErrorResponse),
    )
)]
pub async fn search_handler(
    State(state): State<RestState>,
    Extension(ctx): Extension<SchemaContext>,
    Query(params): Query<SearchParams>,
) -> Response {
    // Bound the work a single request can ask for.
    if params.q.len() > 256 {
        return StatusCode::BAD_REQUEST.into_response();
    }
    if let Some(ct) = params.content_type.as_deref() {
        if !is_known_type(&ctx, ct) {
            return StatusCode::NOT_FOUND.into_response();
        }
    }
    match state
        .store
        .search(
            params.content_type.as_deref(),
            &params.q,
            Pagination {
                page: params.page.unwrap_or(1).max(1),
                per_page: 20,
            },
        )
        .await
    {
        Ok(result) => {
            // The index spans every status and type, so filter per item: unknown types
            // and (for callers without read access) non-published items are dropped.
            let mut items: Vec<ContentItem> = result
                .items
                .into_iter()
                .filter(|i| {
                    is_known_type(&ctx, &i.content_type)
                        && (i.status == "published" || may_see_unpublished(&ctx, &i.content_type))
                })
                .collect();
            for item in &mut items {
                let denied = denied_fields(&ctx, &item.content_type);
                strip_denied(item, &denied);
            }
            let privileged = ctx.is_authenticated();
            (
                StatusCode::OK,
                [(header::CACHE_CONTROL, cache_control(privileged))],
                Json(serde_json::json!({
                    "total": items.len(),
                    "items": items,
                })),
            )
                .into_response()
        }
        Err(CmsError::Validation(_)) => StatusCode::BAD_REQUEST.into_response(),
        Err(e) => {
            tracing::error!("search error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}
