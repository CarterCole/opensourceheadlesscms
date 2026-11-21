//! REST endpoints for site-wide globals.
//!
//! Public (CDN-cacheable): `GET /api/v1/globals`, `GET /api/v1/globals/{slug}`, and the
//! build-time ES module forms `GET /api/v1/globals.js`, `GET /api/v1/globals/{slug}.js`.
//! Admin: `GET /api/admin/globals`, `PUT /api/admin/globals/{slug}`.
use axum::{
    extract::{Extension, Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Json, Response},
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;

use crate::config::GlobalConfig;
use crate::core::{CachedValue, CmsError};
use crate::globals::{render_js_module, ResolvedGlobal};
use crate::http::rest::RestState;
use crate::schema::SchemaContext;

const CACHE_PREFIX: &str = "_globals:";
const JS_CONTENT_TYPE: &str = "text/javascript; charset=utf-8";

fn cache_control(privileged: bool) -> &'static str {
    if privileged {
        "private, no-store"
    } else {
        "public, max-age=60"
    }
}

/// Signed-in readers of the `globals` resource may see non-public globals, so their
/// responses differ from anonymous ones and must not be shared.
fn privileged(ctx: &SchemaContext) -> bool {
    ctx.is_authenticated()
        && ctx.can(
            crate::core::Action::Read,
            crate::schema::context::GLOBALS_RESOURCE,
        )
}

/// Readable definitions, ordered by slug for stable output.
fn readable_defs(ctx: &SchemaContext) -> Vec<&GlobalConfig> {
    let mut defs: Vec<&GlobalConfig> = ctx
        .config()
        .globals
        .values()
        .filter(|d| ctx.can_read_global(d))
        .collect();
    defs.sort_by_key(|d| d.slug());
    defs
}

/// `?slugs=a,b` and/or `?names=c,d` narrow a bulk read; with neither, every readable
/// global is returned. Unknown or unreadable entries are left out, not errors.
#[derive(Deserialize, Default)]
pub struct BulkParams {
    pub slugs: Option<String>,
    pub names: Option<String>,
}

/// Longest selection list accepted in one bulk read.
const MAX_SELECTED: usize = 100;

fn split_list(s: &Option<String>) -> Vec<&str> {
    s.as_deref()
        .map(|s| {
            s.split(',')
                .map(str::trim)
                .filter(|x| !x.is_empty())
                .take(MAX_SELECTED)
                .collect()
        })
        .unwrap_or_default()
}

/// The readable definitions a bulk request asks for, plus a cache-key fragment.
fn select_defs<'a>(ctx: &'a SchemaContext, params: &BulkParams) -> (Vec<&'a GlobalConfig>, String) {
    let (slugs, names) = (split_list(&params.slugs), split_list(&params.names));
    let all = readable_defs(ctx);
    if slugs.is_empty() && names.is_empty() {
        return (all, "*".into());
    }
    let picked: Vec<&GlobalConfig> = all
        .into_iter()
        .filter(|d| slugs.contains(&d.slug().as_str()) || names.contains(&d.name.as_str()))
        .collect();
    let key = picked
        .iter()
        .map(|d| d.slug())
        .collect::<Vec<_>>()
        .join(",");
    (picked, key)
}

fn storage_error(e: CmsError) -> Response {
    tracing::error!("globals: {e}");
    StatusCode::INTERNAL_SERVER_ERROR.into_response()
}

fn detail_json(r: &ResolvedGlobal) -> Value {
    json!({
        "slug": r.def.slug(),
        "name": r.def.name,
        "kind": r.def.kind,
        "item": r.def.item,
        "value": r.value,
        "updated_at": r.stored.as_ref().and_then(|s| s.updated_at),
    })
}

async fn cached_or<F, Fut>(
    state: &RestState,
    key: String,
    privileged: bool,
    content_type: &'static str,
    build: F,
) -> Response
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<String, Response>>,
{
    let headers = |body: String| {
        (
            StatusCode::OK,
            [
                (header::CACHE_CONTROL, cache_control(privileged)),
                (header::CONTENT_TYPE, content_type),
            ],
            body,
        )
            .into_response()
    };
    if let Some(hit) = state.cache.get(&key).await {
        if let Ok(body) = String::from_utf8(hit.0.clone()) {
            return headers(body);
        }
    }
    match build().await {
        Ok(body) => {
            state
                .cache
                .set(
                    &key,
                    CachedValue(body.clone().into_bytes()),
                    Duration::from_secs(60),
                )
                .await;
            headers(body)
        }
        Err(resp) => resp,
    }
}

async fn read_all<'a>(
    ctx: &'a SchemaContext,
    defs: &[&'a GlobalConfig],
) -> Result<Vec<ResolvedGlobal<'a>>, CmsError> {
    let mut out = Vec::with_capacity(defs.len());
    for def in defs {
        out.push(ctx.globals.read(def).await?);
    }
    Ok(out)
}

/// GET /api/v1/globals — every readable global as `{slug: value}`
#[utoipa::path(
    get,
    path = "/api/v1/globals",
    tag = "Globals",
    params(
        ("slugs" = Option<String>, Query, description = "Comma-separated slugs to return (default: all)"),
        ("names" = Option<String>, Query, description = "Comma-separated names to return; combined with `slugs`"),
    ),
    responses((status = 200, description = "Object of global slug → value", body = Object))
)]
pub async fn list_globals_handler(
    State(state): State<RestState>,
    Extension(ctx): Extension<SchemaContext>,
    Query(params): Query<BulkParams>,
) -> Response {
    let privileged = privileged(&ctx);
    let (defs, sel) = select_defs(&ctx, &params);
    let key = format!("{CACHE_PREFIX}all:json:{}:{sel}", privileged as u8);
    cached_or(&state, key, privileged, "application/json", || async {
        let all = read_all(&ctx, &defs).await.map_err(storage_error)?;
        let obj: serde_json::Map<String, Value> =
            all.into_iter().map(|r| (r.def.slug(), r.value)).collect();
        Ok(Value::Object(obj).to_string())
    })
    .await
}

/// GET /api/v1/globals.js — every readable global as an ES module
#[utoipa::path(
    get,
    path = "/api/v1/globals.js",
    tag = "Globals",
    params(
        ("slugs" = Option<String>, Query, description = "Comma-separated slugs to return (default: all)"),
        ("names" = Option<String>, Query, description = "Comma-separated names to return; combined with `slugs`"),
    ),
    responses((status = 200, description = "ES module: `export const <camelSlug> = …` plus a default export keyed by slug", content_type = "text/javascript"))
)]
pub async fn globals_js_handler(
    State(state): State<RestState>,
    Extension(ctx): Extension<SchemaContext>,
    Query(params): Query<BulkParams>,
) -> Response {
    let privileged = privileged(&ctx);
    let (defs, sel) = select_defs(&ctx, &params);
    let key = format!("{CACHE_PREFIX}all:js:{}:{sel}", privileged as u8);
    cached_or(&state, key, privileged, JS_CONTENT_TYPE, || async {
        let all = read_all(&ctx, &defs).await.map_err(storage_error)?;
        let entries: Vec<(&GlobalConfig, &Value)> = all.iter().map(|r| (r.def, &r.value)).collect();
        Ok(render_js_module(&entries))
    })
    .await
}

/// GET /api/v1/globals/{slug} (or `{slug}.js` for an ES module of just that global)
#[utoipa::path(
    get,
    path = "/api/v1/globals/{slug}",
    tag = "Globals",
    params(("slug" = String, Path, description = "Global slug, e.g. `primary-nav`; append `.js` for an ES module")),
    responses(
        (status = 200, description = "`{slug, name, kind, item, value, updated_at}`", body = Object),
        (status = 404, description = "No such global (or not readable by the caller)"),
    )
)]
pub async fn get_global_handler(
    State(state): State<RestState>,
    Extension(ctx): Extension<SchemaContext>,
    Path(slug): Path<String>,
) -> Response {
    let (slug, as_js) = match slug.strip_suffix(".js") {
        Some(s) => (s.to_string(), true),
        None => (slug, false),
    };
    // Unreadable globals 404 rather than 403 so their existence isn't revealed.
    let Some(def) = ctx
        .config()
        .global_by_slug(&slug)
        .filter(|d| ctx.can_read_global(d))
    else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let privileged = privileged(&ctx);
    let key = format!(
        "{CACHE_PREFIX}one:{slug}:{}:{}",
        if as_js { "js" } else { "json" },
        privileged as u8
    );
    let content_type = if as_js {
        JS_CONTENT_TYPE
    } else {
        "application/json"
    };
    cached_or(&state, key, privileged, content_type, || async {
        let r = ctx.globals.read(def).await.map_err(storage_error)?;
        Ok(if as_js {
            render_js_module(&[(r.def, &r.value)])
        } else {
            detail_json(&r).to_string()
        })
    })
    .await
}

fn admin_json(r: &ResolvedGlobal, default_storage: crate::config::GlobalStorage) -> Value {
    json!({
        "slug": r.def.slug(),
        "name": r.def.name,
        "label": r.def.display_label(),
        "description": r.def.description,
        "kind": r.def.kind,
        "item": r.def.item,
        "public": r.def.public,
        "storage": r.def.storage.unwrap_or(default_storage),
        "max_items": r.def.max_items,
        "value": r.value,
        "updated_at": r.stored.as_ref().and_then(|s| s.updated_at),
        "updated_by": r.stored.as_ref().and_then(|s| s.updated_by.clone()),
    })
}

/// GET /api/admin/globals — definitions plus current values, for the admin UI
pub async fn admin_list_globals_handler(Extension(ctx): Extension<SchemaContext>) -> Response {
    if !ctx.is_authenticated() {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let defs = readable_defs(&ctx);
    let all = match read_all(&ctx, &defs).await {
        Ok(a) => a,
        Err(e) => return storage_error(e),
    };
    let default_storage = ctx.config().server.globals.storage;
    let items: Vec<Value> = all.iter().map(|r| admin_json(r, default_storage)).collect();
    (
        [(header::CACHE_CONTROL, "private, no-store")],
        Json(json!({ "globals": items, "can_edit": ctx.can_write_globals() })),
    )
        .into_response()
}

#[derive(Deserialize)]
pub struct PutGlobalBody {
    pub value: Value,
}

/// PUT /api/admin/globals/{slug} — body `{"value": …}`
pub async fn admin_put_global_handler(
    State(state): State<RestState>,
    Extension(ctx): Extension<SchemaContext>,
    Path(slug): Path<String>,
    Json(body): Json<PutGlobalBody>,
) -> Response {
    let Some(user_id) = ctx.user_id.clone() else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let Some(def) = ctx.config().global_by_slug(&slug) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !ctx.can_write_globals() {
        return StatusCode::FORBIDDEN.into_response();
    }
    match ctx.globals.write(def, body.value, &user_id).await {
        Ok(r) => {
            state.cache.invalidate_prefix(CACHE_PREFIX).await;
            let default_storage = ctx.config().server.globals.storage;
            (
                [(header::CACHE_CONTROL, "private, no-store")],
                Json(admin_json(&r, default_storage)),
            )
                .into_response()
        }
        Err(CmsError::Validation(msg)) => {
            (StatusCode::BAD_REQUEST, Json(json!({ "error": msg }))).into_response()
        }
        Err(e) => storage_error(e),
    }
}
