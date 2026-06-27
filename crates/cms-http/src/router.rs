use async_graphql::dynamic::Schema;
use axum::{
    middleware,
    routing::{get, post},
    Router,
};
use cms_core::{CacheLayer, ContentStore};
use cms_schema::SchemaContext;
use std::sync::Arc;
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use crate::{
    auth::{login_handler, AuthState},
    graphql::{graphql_handler, graphql_playground_handler},
    middleware::{jwt_auth_middleware, AuthMiddlewareState},
    rest::{get_by_slug_handler, health_handler, list_handler, search_handler, RestState},
};

pub struct RouterConfig {
    pub store: Arc<dyn ContentStore>,
    pub cache: Arc<dyn CacheLayer>,
    pub schema: Schema,
    pub ctx_template: SchemaContext,
    pub jwt_secret: String,
    pub dev_mode: bool,
}

pub fn build_router(cfg: RouterConfig) -> Router {
    let auth_state = AuthState {
        store: cfg.store.clone(),
        jwt_secret: cfg.jwt_secret.clone(),
    };

    let rest_state = RestState {
        store: cfg.store.clone(),
        cache: cfg.cache.clone(),
    };

    let auth_mw_state = AuthMiddlewareState {
        jwt_secret: cfg.jwt_secret.clone(),
        ctx_template: cfg.ctx_template.clone(),
    };

    let mut graphql_router = Router::new()
        .route("/graphql", post(graphql_handler));
    if cfg.dev_mode {
        graphql_router = graphql_router.route("/graphql", get(graphql_playground_handler));
    }
    let graphql_router = graphql_router.with_state(cfg.schema);

    let public_api = Router::new()
        .route("/api/v1/search", get(search_handler))
        .route("/api/v1/:type", get(list_handler))
        .route("/api/v1/:type/:slug", get(get_by_slug_handler))
        .with_state(rest_state);

    let auth_router = Router::new()
        .route("/api/auth/login", post(login_handler))
        .with_state(auth_state);

    Router::new()
        .route("/health", get(health_handler))
        .merge(auth_router)
        .merge(public_api)
        .merge(graphql_router)
        .layer(middleware::from_fn_with_state(auth_mw_state, jwt_auth_middleware))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
}
