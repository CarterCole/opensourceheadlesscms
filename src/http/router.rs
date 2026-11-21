use crate::config::{OidcConfig, RateLimitConfig, SecurityConfig};
use crate::core::{CacheLayer, ContentStore};
use crate::schema::SchemaContext;
use async_graphql::dynamic::Schema;
use axum::{
    extract::DefaultBodyLimit,
    http::{header, HeaderName, HeaderValue, Method},
    middleware,
    routing::{get, post, put},
    Router,
};
use std::sync::Arc;
use std::time::Duration;
use tower_http::{
    cors::{AllowOrigin, Any, CorsLayer},
    services::ServeDir,
    set_header::SetResponseHeaderLayer,
    trace::TraceLayer,
};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use crate::http::{
    auth::{login_handler, logout_handler, me_handler, AuthState},
    globals::{
        admin_list_globals_handler, admin_put_global_handler, get_global_handler,
        globals_js_handler, list_globals_handler,
    },
    graphql::{graphql_handler, graphql_playground_handler},
    middleware::{jwt_auth_middleware, AuthMiddlewareState},
    oidc::{self, OidcClient, OidcState},
    openapi::ApiDoc,
    ratelimit::{rate_limit_middleware, LoginThrottle, RateLimitState},
    rest::{
        admin_config_handler, get_by_slug_handler, health_handler, list_handler, search_handler,
        RestState,
    },
    subscriptions::{graphql_ws_handler, GraphQLWsState},
};

pub struct RouterConfig {
    pub store: Arc<dyn ContentStore>,
    pub cache: Arc<dyn CacheLayer>,
    pub schema: Schema,
    pub ctx_template: SchemaContext,
    pub jwt_secret: String,
    pub dev_mode: bool,
    /// Path to frontend/dist for serving the admin UI (None = skip)
    pub admin_ui_dir: Option<String>,
    pub security: SecurityConfig,
    pub rate_limit: RateLimitConfig,
    /// "Sign in with Kith" / OpenID Connect login (None = off).
    pub oidc: Option<OidcConfig>,
}

/// Cross-origin policy. Empty list = same-origin only (no CORS headers at all).
/// Credentials are never allowed cross-origin: other sites must use a bearer token.
fn cors_layer(origins: &[String]) -> Option<CorsLayer> {
    if origins.is_empty() {
        return None;
    }
    let base = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE])
        .max_age(Duration::from_secs(3600));
    if origins.iter().any(|o| o == "*") {
        return Some(base.allow_origin(Any));
    }
    let parsed: Vec<HeaderValue> = origins
        .iter()
        .filter_map(|o| HeaderValue::from_str(o.trim_end_matches('/')).ok())
        .collect();
    Some(base.allow_origin(AllowOrigin::list(parsed)))
}

/// Strict policy for the admin UI bundle (no inline scripts, no third-party origins).
const ADMIN_CSP: &str = "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; \
img-src 'self' data: https:; font-src 'self' data:; connect-src 'self' ws: wss:; \
object-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'";

pub fn build_router(cfg: RouterConfig) -> Router {
    let cookie_secure = cfg.security.cookie_secure.unwrap_or(!cfg.dev_mode);

    let auth_state = AuthState {
        store: cfg.store.clone(),
        jwt_secret: cfg.jwt_secret.clone(),
        jwt_ttl_seconds: cfg.security.jwt_ttl_seconds,
        cookie_secure,
        throttle: Arc::new(LoginThrottle::new(&cfg.security)),
    };

    let rest_state = RestState {
        store: cfg.store.clone(),
        cache: cfg.cache.clone(),
    };

    let auth_mw_state = AuthMiddlewareState {
        jwt_secret: cfg.jwt_secret.clone(),
        ctx_template: cfg.ctx_template.clone(),
        trusted_origins: cfg
            .security
            .cors_allowed_origins
            .iter()
            .map(|o| o.trim_end_matches('/').to_string())
            .collect(),
    };

    let rate_limit_state = RateLimitState::new(cfg.rate_limit.clone(), &cfg.security);

    let mut graphql_router = Router::new().route("/graphql", post(graphql_handler));
    if cfg.dev_mode {
        graphql_router = graphql_router.route("/graphql", get(graphql_playground_handler));
    }
    let graphql_router = graphql_router.with_state(cfg.schema.clone());

    let ws_state = GraphQLWsState {
        schema: cfg.schema,
        ctx_template: cfg.ctx_template.clone(),
        jwt_secret: cfg.jwt_secret.clone(),
    };
    let graphql_ws_router = Router::new()
        .route("/graphql/ws", get(graphql_ws_handler))
        .with_state(ws_state);

    let public_api = Router::new()
        .route("/api/v1/search", get(search_handler))
        // Static segments win over `/api/v1/{type}`, so these shadow a type named `globals`.
        .route("/api/v1/globals", get(list_globals_handler))
        .route("/api/v1/globals.js", get(globals_js_handler))
        .route("/api/v1/globals/{slug}", get(get_global_handler))
        .route("/api/v1/{type}", get(list_handler))
        .route("/api/v1/{type}/{slug}", get(get_by_slug_handler))
        .with_state(rest_state);

    let auth_router = Router::new()
        .route("/api/auth/login", post(login_handler))
        .route("/api/auth/logout", post(logout_handler))
        .route("/api/auth/me", get(me_handler))
        .with_state(auth_state);

    let oidc_client = cfg.oidc.clone().and_then(|c| match OidcClient::new(c) {
        Ok(client) => Some(Arc::new(client)),
        Err(e) => {
            tracing::error!("oidc disabled: {e}");
            None
        }
    });
    let oidc_router = oidc::router(OidcState {
        client: oidc_client,
        store: cfg.store.clone(),
        jwt_secret: cfg.jwt_secret.clone(),
        jwt_ttl_seconds: cfg.security.jwt_ttl_seconds,
        cookie_secure,
    });

    let admin_api = Router::new()
        .route("/api/admin/config", get(admin_config_handler))
        .route("/api/admin/globals", get(admin_list_globals_handler))
        .route("/api/admin/globals/{slug}", put(admin_put_global_handler))
        .with_state(RestState {
            store: cfg.store.clone(),
            cache: cfg.cache.clone(),
        });

    let swagger = SwaggerUi::new("/api/docs").url("/api/openapi.json", ApiDoc::openapi());

    // Layers added first sit closest to the handlers; the last added is outermost.
    let mut router = Router::new()
        .route("/health", get(health_handler))
        .merge(swagger)
        .merge(auth_router)
        .merge(oidc_router)
        .merge(public_api)
        .merge(admin_api)
        .merge(graphql_router)
        .merge(graphql_ws_router)
        .layer(middleware::from_fn_with_state(
            auth_mw_state,
            jwt_auth_middleware,
        ))
        .layer(middleware::from_fn_with_state(
            rate_limit_state,
            rate_limit_middleware,
        ))
        .layer(DefaultBodyLimit::max(1024 * 1024));

    if let Some(cors) = cors_layer(&cfg.security.cors_allowed_origins) {
        router = router.layer(cors);
    }

    // Serve admin UI static files at /admin/* if the dist dir is present
    if let Some(dir) = cfg.admin_ui_dir {
        if std::path::Path::new(&dir).exists() {
            let admin_ui = Router::new()
                .nest_service(
                    "/admin",
                    ServeDir::new(dir).append_index_html_on_directories(true),
                )
                .layer(SetResponseHeaderLayer::overriding(
                    header::CONTENT_SECURITY_POLICY,
                    HeaderValue::from_static(ADMIN_CSP),
                ));
            router = router.merge(admin_ui);
        }
    }

    router
        .layer(SetResponseHeaderLayer::if_not_present(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::X_FRAME_OPTIONS,
            HeaderValue::from_static("DENY"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            HeaderName::from_static("referrer-policy"),
            HeaderValue::from_static("no-referrer"),
        ))
        .layer(TraceLayer::new_for_http())
}
