use argon2::{password_hash::SaltString, Argon2, PasswordHasher};
use chrono::Utc;
use opensourceheadlesscms::cache::{MokaCache, TwoLayerCache};
use opensourceheadlesscms::config::{ConfigLoader, DatabaseDriver};
use opensourceheadlesscms::core::Pagination;
use opensourceheadlesscms::db::{PostgresStore, SqliteStore};
use opensourceheadlesscms::http::{build_router, router::RouterConfig};
use opensourceheadlesscms::rbac::RbacEngine;
use opensourceheadlesscms::schema::{build_schema, SchemaContext};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

/// The placeholder shipped in the example configs. Anyone can read it in the
/// public repository, so it must never be accepted as a real signing key.
const PLACEHOLDER_JWT_SECRET: &str = "changeme-use-a-real-secret-in-production";
const MIN_JWT_SECRET_LEN: usize = 32;

/// Refuse to run with a guessable JWT signing secret.
///
/// A known secret lets anyone forge an admin token. In `development` a random
/// per-process secret is generated instead (sessions reset on restart); in any
/// other environment the server refuses to start.
fn resolve_jwt_secret(
    server: &mut opensourceheadlesscms::config::ServerConfig,
) -> anyhow::Result<()> {
    use rand_core::{OsRng, RngCore};

    let secret = server.jwt_secret.trim();
    let weak =
        secret.is_empty() || secret == PLACEHOLDER_JWT_SECRET || secret.len() < MIN_JWT_SECRET_LEN;
    if !weak {
        return Ok(());
    }

    if server.env == "development" {
        let mut bytes = [0u8; 32];
        OsRng.fill_bytes(&mut bytes);
        server.jwt_secret = bytes.iter().map(|b| format!("{b:02x}")).collect();
        warn!(
            "jwt_secret is missing, a placeholder, or shorter than {MIN_JWT_SECRET_LEN} chars; \
             using a random ephemeral secret (logins reset on restart). \
             Set CMS_JWT_SECRET to a long random value for anything real."
        );
        Ok(())
    } else {
        anyhow::bail!(
            "jwt_secret is missing, the shipped placeholder, or shorter than {MIN_JWT_SECRET_LEN} \
             characters. Set CMS_JWT_SECRET (e.g. `openssl rand -hex 32`)."
        )
    }
}

/// 24-char alphanumeric password from the OS CSPRNG (unbiased rejection sampling).
fn generate_password() -> String {
    use rand_core::{OsRng, RngCore};
    const CHARSET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789";
    let limit = (256 / CHARSET.len() * CHARSET.len()) as u8;
    let mut out = String::with_capacity(24);
    while out.len() < 24 {
        let mut b = [0u8; 1];
        OsRng.fill_bytes(&mut b);
        if b[0] < limit {
            out.push(CHARSET[b[0] as usize % CHARSET.len()] as char);
        }
    }
    out
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if std::env::args().any(|a| a == "--print-schema") {
        return print_schema().await;
    }

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("cms=info".parse()?))
        .init();

    let config_path = std::env::var("CMS_CONFIG_PATH").unwrap_or_else(|_| "cms.toml".to_string());

    let loader = ConfigLoader::new(&config_path);
    let mut config = loader
        .load()
        .map_err(|e| anyhow::anyhow!("config load failed: {e}"))?;

    if let Ok(secret) = std::env::var("CMS_JWT_SECRET") {
        config.server.jwt_secret = secret;
    }
    if let Ok(url) = std::env::var("CMS_DATABASE_URL") {
        config.server.database.url = Some(url);
    }
    if let Ok(secret) = std::env::var("CMS_OIDC_CLIENT_SECRET") {
        if let Some(oidc) = config.server.oidc.as_mut() {
            oidc.client_secret = Some(secret);
        }
    }
    if let Ok(url) = std::env::var("CMS_GLOBALS_REDIS_URL") {
        config.server.globals.redis_url = Some(url);
    }
    resolve_jwt_secret(&mut config.server)?;
    if let Some(oidc) = &config.server.oidc {
        oidc.validate()
            .map_err(|e| anyhow::anyhow!("invalid [oidc] config: {e}"))?;
        info!("oidc sign-in enabled ({})", oidc.issuer);
    }

    info!(
        "loaded: {} content types, {} globals, {} roles",
        config.content_types.len(),
        config.globals.len(),
        config.roles.len()
    );

    let store: Arc<dyn opensourceheadlesscms::core::ContentStore> =
        match config.server.database.driver {
            DatabaseDriver::Postgres => {
                let url = config
                    .server
                    .database
                    .url
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("postgres: database.url is required"))?;
                Arc::new(
                    PostgresStore::new(url, "migrations/postgres")
                        .await
                        .map_err(|e| anyhow::anyhow!("db: {e}"))?,
                )
            }
            DatabaseDriver::Sqlite => {
                let url = config
                    .server
                    .database
                    .url
                    .as_deref()
                    .unwrap_or("sqlite://cms.db");
                Arc::new(
                    SqliteStore::new(url, "migrations/sqlite")
                        .await
                        .map_err(|e| anyhow::anyhow!("db: {e}"))?,
                )
            }
            DatabaseDriver::D1 => anyhow::bail!("D1 driver is not yet implemented"),
        };

    if config.server.database.auto_migrate {
        store
            .run_migrations()
            .await
            .map_err(|e| anyhow::anyhow!("migrations: {e}"))?;
        info!("migrations applied");
    }

    // First-run: seed an admin account if no users exist
    seed_first_admin(&store).await?;

    let l1 = MokaCache::new(
        config.server.cache.l1_max_capacity,
        Duration::from_secs(config.server.cache.l1_ttl_seconds),
    );
    let cache: Arc<dyn opensourceheadlesscms::core::CacheLayer> = Arc::new(TwoLayerCache::new(l1));
    let rbac: Arc<dyn opensourceheadlesscms::core::PermissionEngine> =
        Arc::new(RbacEngine::new(&config));
    let events = Arc::new(opensourceheadlesscms::core::EventBus::new());

    let globals =
        opensourceheadlesscms::globals::GlobalsService::from_config(&config, store.clone())
            .await
            .map_err(|e| anyhow::anyhow!("globals storage: {e}"))?;
    let ctx_template = SchemaContext::new(
        store.clone(),
        cache.clone(),
        rbac.clone(),
        Arc::new(config.clone()),
        events.clone(),
    )
    .with_globals(Arc::new(globals));

    let schema =
        build_schema(&config, ctx_template.clone()).map_err(|e| anyhow::anyhow!("schema: {e}"))?;
    info!("graphql schema built");

    // Scheduled publishing background task
    let sched_store = store.clone();
    let sched_cache = cache.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        loop {
            interval.tick().await;
            match sched_store.list_scheduled_due(Utc::now()).await {
                Ok(items) => {
                    for item in items {
                        if let Err(e) = sched_store
                            .transition(&item.content_type, &item.id, "published", "scheduler")
                            .await
                        {
                            error!("auto-publish failed {}/{}: {e}", item.content_type, item.id);
                        } else {
                            info!("auto-published {}/{}", item.content_type, item.id);
                            sched_cache.invalidate_prefix(&item.content_type).await;
                        }
                    }
                }
                Err(e) => error!("scheduler query: {e}"),
            }
        }
    });

    let dev_mode = config.server.env == "development";
    let admin_ui_dir =
        std::env::var("CMS_ADMIN_UI_DIR").unwrap_or_else(|_| "frontend/dist".to_string());
    let router = build_router(RouterConfig {
        store,
        cache,
        schema,
        ctx_template,
        jwt_secret: config.server.jwt_secret.clone(),
        dev_mode,
        admin_ui_dir: Some(admin_ui_dir),
        security: config.server.security.clone(),
        rate_limit: config.server.rate_limit.clone(),
        oidc: config.server.oidc.clone(),
    });

    let addr = format!("0.0.0.0:{}", config.server.http_port);
    let listener = TcpListener::bind(&addr).await?;
    info!("listening on http://{addr}");
    if dev_mode {
        info!("playground: http://{addr}/graphql");
    }

    // ConnectInfo gives the rate limiter the real peer address.
    axum::serve(
        listener,
        router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await?;
    Ok(())
}

/// Prints the GraphQL schema SDL to stdout and exits, without connecting to
/// the configured database. Used by `frontend`'s `relay-schema` npm script to
/// feed relay-compiler, since it needs a static schema file to compile against.
async fn print_schema() -> anyhow::Result<()> {
    let config_path = std::env::var("CMS_CONFIG_PATH").unwrap_or_else(|_| "cms.toml".to_string());
    let loader = ConfigLoader::new(&config_path);
    let config = loader
        .load()
        .map_err(|e| anyhow::anyhow!("config load failed: {e}"))?;

    let store: Arc<dyn opensourceheadlesscms::core::ContentStore> = Arc::new(
        SqliteStore::new("sqlite://:memory:", "migrations/sqlite")
            .await
            .map_err(|e| anyhow::anyhow!("db: {e}"))?,
    );
    let cache: Arc<dyn opensourceheadlesscms::core::CacheLayer> = Arc::new(TwoLayerCache::new(
        MokaCache::new(10, Duration::from_secs(1)),
    ));
    let rbac: Arc<dyn opensourceheadlesscms::core::PermissionEngine> =
        Arc::new(RbacEngine::new(&config));
    let events = Arc::new(opensourceheadlesscms::core::EventBus::new());

    let ctx_template = SchemaContext::new(store, cache, rbac, Arc::new(config.clone()), events);
    let schema = build_schema(&config, ctx_template).map_err(|e| anyhow::anyhow!("schema: {e}"))?;

    println!("{}", schema.sdl());
    Ok(())
}

async fn seed_first_admin(
    store: &Arc<dyn opensourceheadlesscms::core::ContentStore>,
) -> anyhow::Result<()> {
    let users = store
        .list_users(Pagination {
            page: 1,
            per_page: 1,
        })
        .await
        .map_err(|e| anyhow::anyhow!("list_users: {e}"))?;
    if !users.is_empty() {
        return Ok(());
    }

    let password = generate_password();
    let salt = SaltString::generate(&mut argon2::password_hash::rand_core::OsRng);
    let hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("argon2: {e}"))?
        .to_string();

    store
        .create_user("admin@localhost", "Admin", &hash, vec!["admin".to_string()])
        .await
        .map_err(|e| anyhow::anyhow!("create_user: {e}"))?;

    println!();
    println!("╔══════════════════════════════════════════════════╗");
    println!("║            CMS FIRST RUN — ADMIN CREATED         ║");
    println!("║  Email:    admin@localhost                       ║");
    println!("║  Password: {password:<38}║");
    println!("║  Change this password after first login!         ║");
    println!("╚══════════════════════════════════════════════════╝");
    println!();

    Ok(())
}
