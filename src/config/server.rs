use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_http_port")]
    pub http_port: u16,
    #[serde(default = "default_grpc_port")]
    pub grpc_port: u16,
    pub jwt_secret: String,
    #[serde(default = "default_env")]
    pub env: String,
    pub database: DatabaseConfig,
    #[serde(default)]
    pub cache: CacheConfig,
    #[serde(default)]
    pub flatfile: FlatfileConfig,
    #[serde(default)]
    pub config_dirs: ConfigDirs,
    #[serde(default)]
    pub webhooks: Vec<WebhookConfig>,
    #[serde(default)]
    pub rate_limit: RateLimitConfig,
    #[serde(default)]
    pub media: MediaConfig,
    #[serde(default)]
    pub security: SecurityConfig,
}

fn default_http_port() -> u16 {
    8080
}
fn default_grpc_port() -> u16 {
    50051
}
fn default_env() -> String {
    // Secure by default: dev conveniences (GraphQL playground, ephemeral JWT
    // secret) must be opted into with `env = "development"`.
    "production".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    pub driver: DatabaseDriver,
    pub url: Option<String>,
    #[serde(default)]
    pub auto_migrate: bool,
    // D1
    pub cf_account_id: Option<String>,
    pub cf_database_id: Option<String>,
    pub cf_api_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DatabaseDriver {
    Sqlite,
    Postgres,
    D1,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CacheConfig {
    #[serde(default = "default_l1_capacity")]
    pub l1_max_capacity: u64,
    #[serde(default = "default_l1_ttl")]
    pub l1_ttl_seconds: u64,
    #[serde(default)]
    pub l2_enabled: bool,
    pub l2_redis_url: Option<String>,
    #[serde(default = "default_l2_ttl")]
    pub l2_ttl_seconds: u64,
}

fn default_l1_capacity() -> u64 {
    10_000
}
fn default_l1_ttl() -> u64 {
    5
}
fn default_l2_ttl() -> u64 {
    60
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FlatfileConfig {
    #[serde(default = "default_content_dir")]
    pub content_dir: String,
    #[serde(default = "default_true")]
    pub git_auto_commit: bool,
    #[serde(default = "default_git_author_name")]
    pub git_author_name: String,
    #[serde(default = "default_git_author_email")]
    pub git_author_email: String,
}

fn default_content_dir() -> String {
    "./content".to_string()
}
fn default_true() -> bool {
    true
}
fn default_git_author_name() -> String {
    "CMS Bot".to_string()
}
fn default_git_author_email() -> String {
    "cms@example.com".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConfigDirs {
    #[serde(default = "default_content_types_dir")]
    pub content_types: String,
    #[serde(default = "default_workflows_dir")]
    pub workflows: String,
    #[serde(default = "default_roles_dir")]
    pub roles: String,
}

fn default_content_types_dir() -> String {
    "./content-types".to_string()
}
fn default_workflows_dir() -> String {
    "./workflows".to_string()
}
fn default_roles_dir() -> String {
    "./roles".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookConfig {
    pub url: String,
    pub events: Vec<String>,
    pub secret: Option<String>,
    #[serde(default)]
    pub content_types: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    #[serde(default = "default_public_rps")]
    pub public_rps: u32,
    #[serde(default = "default_graphql_rps")]
    pub graphql_rps: u32,
    #[serde(default = "default_auth_rps")]
    pub auth_rps: u32,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            public_rps: 100,
            graphql_rps: 30,
            auth_rps: 5,
        }
    }
}

fn default_public_rps() -> u32 {
    100
}
fn default_graphql_rps() -> u32 {
    30
}
fn default_auth_rps() -> u32 {
    5
}

/// Authentication, session and cross-origin settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// Origins allowed to call the API cross-origin from a browser. Empty (the default)
    /// means same-origin only; `["*"]` allows any origin (credentials are never allowed
    /// cross-origin, so such callers must use a bearer token).
    #[serde(default)]
    pub cors_allowed_origins: Vec<String>,
    /// Lifetime of issued session tokens, in seconds.
    #[serde(default = "default_jwt_ttl")]
    pub jwt_ttl_seconds: i64,
    /// Mark the session cookie `Secure`. `None` = on unless `env = "development"`.
    #[serde(default)]
    pub cookie_secure: Option<bool>,
    /// Trust `X-Forwarded-For` / `X-Real-IP` for the client address. Enable only behind a
    /// reverse proxy that overwrites those headers; otherwise clients can spoof them.
    #[serde(default)]
    pub trust_proxy_headers: bool,
    /// Failed logins allowed per client address before it is locked out.
    #[serde(default = "default_login_max_failures")]
    pub login_max_failures: u32,
    /// Failed logins allowed per account (from any address) before it is locked out.
    #[serde(default = "default_login_account_max_failures")]
    pub login_account_max_failures: u32,
    /// Window for counting failures, and how long a lockout lasts, in seconds.
    #[serde(default = "default_login_lockout")]
    pub login_lockout_seconds: u64,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            cors_allowed_origins: vec![],
            jwt_ttl_seconds: default_jwt_ttl(),
            cookie_secure: None,
            trust_proxy_headers: false,
            login_max_failures: default_login_max_failures(),
            login_account_max_failures: default_login_account_max_failures(),
            login_lockout_seconds: default_login_lockout(),
        }
    }
}

fn default_jwt_ttl() -> i64 {
    8 * 60 * 60
}
fn default_login_max_failures() -> u32 {
    5
}
fn default_login_account_max_failures() -> u32 {
    20
}
fn default_login_lockout() -> u64 {
    15 * 60
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MediaConfig {
    #[serde(default = "default_media_storage")]
    pub storage: String,
    #[serde(default = "default_media_dir")]
    pub local_dir: String,
    pub s3_bucket: Option<String>,
    pub s3_region: Option<String>,
}

fn default_media_storage() -> String {
    "local".to_string()
}
fn default_media_dir() -> String {
    "./media".to_string()
}
