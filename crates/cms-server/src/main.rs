use argon2::{password_hash::SaltString, Argon2, PasswordHasher};
use chrono::Utc;
use cms_cache::{MokaCache, TwoLayerCache};
use cms_core::Pagination;
use cms_config::ConfigLoader;
use cms_db::SqliteStore;
use cms_http::{build_router, router::RouterConfig};
use cms_rbac::RbacEngine;
use cms_schema::{build_schema, SchemaContext};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("cms=info".parse()?))
        .init();

    let config_path = std::env::var("CMS_CONFIG_PATH")
        .unwrap_or_else(|_| "cms.toml".to_string());

    let loader = ConfigLoader::new(&config_path);
    let config = match loader.load() {
        Ok(c) => c,
        Err(e) => {
            error!("config load failed ({e}), using defaults");
            build_default_config()
        }
    };

    info!("loaded: {} content types, {} roles",
        config.content_types.len(), config.roles.len());

    let db_url = config.server.database.url
        .as_deref()
        .unwrap_or("sqlite://cms.db");
    let store: Arc<dyn cms_core::ContentStore> = Arc::new(
        SqliteStore::new(db_url, "migrations/sqlite").await
            .map_err(|e| anyhow::anyhow!("db: {e}"))?
    );

    if config.server.database.auto_migrate {
        store.run_migrations().await
            .map_err(|e| anyhow::anyhow!("migrations: {e}"))?;
        info!("migrations applied");
    }

    // First-run: seed an admin account if no users exist
    seed_first_admin(&store).await?;

    let l1 = MokaCache::new(
        config.server.cache.l1_max_capacity,
        Duration::from_secs(config.server.cache.l1_ttl_seconds),
    );
    let cache: Arc<dyn cms_core::CacheLayer> = Arc::new(TwoLayerCache::new(l1));
    let rbac: Arc<dyn cms_core::PermissionEngine> = Arc::new(RbacEngine::new(&config));

    let ctx_template = SchemaContext::new(
        store.clone(),
        cache.clone(),
        rbac.clone(),
        Arc::new(config.clone()),
    );

    let schema = build_schema(&config, ctx_template.clone())
        .map_err(|e| anyhow::anyhow!("schema: {e}"))?;
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
                        if let Err(e) = sched_store.transition(
                            &item.content_type, &item.id, "published", "scheduler",
                        ).await {
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
    let admin_ui_dir = std::env::var("CMS_ADMIN_UI_DIR")
        .unwrap_or_else(|_| "frontend/dist".to_string());
    let router = build_router(RouterConfig {
        store,
        cache,
        schema,
        ctx_template,
        jwt_secret: config.server.jwt_secret.clone(),
        dev_mode,
        admin_ui_dir: Some(admin_ui_dir),
    });

    let addr = format!("0.0.0.0:{}", config.server.http_port);
    let listener = TcpListener::bind(&addr).await?;
    info!("listening on http://{addr}");
    if dev_mode {
        info!("playground: http://{addr}/graphql");
    }

    axum::serve(listener, router).await?;
    Ok(())
}

async fn seed_first_admin(store: &Arc<dyn cms_core::ContentStore>) -> anyhow::Result<()> {
    let users = store.list_users(Pagination { page: 1, per_page: 1 }).await
        .map_err(|e| anyhow::anyhow!("list_users: {e}"))?;
    if !users.is_empty() {
        return Ok(());
    }

    let password = uuid::Uuid::new_v4().to_string().replace('-', "")[..16].to_string();
    let salt = SaltString::generate(&mut argon2::password_hash::rand_core::OsRng);
    let hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("argon2: {e}"))?
        .to_string();

    store.create_user("admin@localhost", "Admin", &hash, vec!["admin".to_string()]).await
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

fn build_default_config() -> cms_config::CmsConfig {
    use cms_config::*;
    use std::collections::HashMap;
    CmsConfig {
        server: ServerConfig {
            http_port: 8080,
            grpc_port: 50051,
            jwt_secret: "dev-secret-change-me".to_string(),
            env: "development".to_string(),
            database: DatabaseConfig {
                driver: DatabaseDriver::Sqlite,
                url: Some("sqlite://cms.db".to_string()),
                auto_migrate: true,
                cf_account_id: None,
                cf_database_id: None,
                cf_api_token: None,
            },
            cache: CacheConfig::default(),
            flatfile: FlatfileConfig::default(),
            config_dirs: ConfigDirs::default(),
            webhooks: vec![],
            rate_limit: RateLimitConfig::default(),
            media: MediaConfig::default(),
        },
        content_types: HashMap::new(),
        workflows: HashMap::new(),
        roles: HashMap::new(),
    }
}
