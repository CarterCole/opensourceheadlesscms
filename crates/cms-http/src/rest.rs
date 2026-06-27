use axum::{
    extract::{Extension, Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Json, Response},
};
use cms_core::{CacheLayer, CachedValue, CmsError, ContentStore, ListQuery, Pagination};
use cms_schema::SchemaContext;
use serde::Deserialize;
use std::sync::Arc;
use std::time::Duration;

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
pub async fn list_handler(
    State(state): State<RestState>,
    Path(content_type): Path<String>,
    Query(params): Query<ListParams>,
) -> Response {
    let page = params.page.unwrap_or(1);
    let per_page = params.per_page.unwrap_or(20).min(100);
    let cache_key = format!("{content_type}:list:{page}:{per_page}:{}", params.status.as_deref().unwrap_or("published"));

    if let Some(cached) = state.cache.get(&cache_key).await {
        if let Ok(json) = cached.to_json::<serde_json::Value>() {
            return (
                StatusCode::OK,
                [(header::CACHE_CONTROL, "public, max-age=60")],
                Json(json),
            ).into_response();
        }
    }

    let query = ListQuery {
        status: Some(params.status.unwrap_or_else(|| "published".to_string())),
        pagination: Some(Pagination { page, per_page }),
        ..Default::default()
    };

    match state.store.list(&content_type, query).await {
        Ok(result) => {
            let json = serde_json::json!({
                "items": result.items,
                "total": result.total,
                "page": result.page,
                "per_page": result.per_page,
            });
            if let Ok(cached) = CachedValue::from_json(&json) {
                state.cache.set(&cache_key, cached, Duration::from_secs(60)).await;
            }
            (
                StatusCode::OK,
                [(header::CACHE_CONTROL, "public, max-age=60")],
                Json(json),
            ).into_response()
        }
        Err(CmsError::NotFound(_)) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            tracing::error!("list error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// GET /api/v1/:type/:slug — single content item (public, CDN-cacheable)
pub async fn get_by_slug_handler(
    State(state): State<RestState>,
    Path((content_type, slug)): Path<(String, String)>,
) -> Response {
    let cache_key = format!("{content_type}:slug:{slug}");

    if let Some(cached) = state.cache.get(&cache_key).await {
        if let Ok(json) = cached.to_json::<serde_json::Value>() {
            return (
                StatusCode::OK,
                [(header::CACHE_CONTROL, "public, max-age=60")],
                Json(json),
            ).into_response();
        }
    }

    match state.store.get_by_slug(&content_type, &slug).await {
        Ok(item) => {
            let json = serde_json::to_value(&item).unwrap_or_default();
            if let Ok(cached) = CachedValue::from_json(&json) {
                state.cache.set(&cache_key, cached, Duration::from_secs(60)).await;
            }
            (
                StatusCode::OK,
                [(header::CACHE_CONTROL, "public, max-age=60")],
                Json(json),
            ).into_response()
        }
        Err(CmsError::NotFound(_)) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            tracing::error!("get_by_slug error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// GET /health
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

pub async fn search_handler(
    State(state): State<RestState>,
    Query(params): Query<SearchParams>,
) -> Response {
    match state.store.search(
        params.content_type.as_deref(),
        &params.q,
        Pagination { page: params.page.unwrap_or(1), per_page: 20 },
    ).await {
        Ok(result) => Json(serde_json::json!({
            "items": result.items,
            "total": result.total,
        })).into_response(),
        Err(e) => {
            tracing::error!("search error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}
