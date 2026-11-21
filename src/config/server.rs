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
    /// "Sign in with Kith" (or any OpenID Connect provider) for the admin UI. Absent = off.
    #[serde(default)]
    pub oidc: Option<OidcConfig>,
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

/// OpenID Connect single sign-on, tested against a Kith hub (`kith hub serve`) but standard:
/// authorization code flow with PKCE, ID tokens signed with RS256.
///
/// The provider must have this server registered as a client whose redirect URI is
/// [`OidcConfig::redirect_uri`]. A successful sign-in ends in the same session cookie as
/// the password login, so everything downstream (roles, GraphQL, logout) is unchanged.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OidcConfig {
    /// The provider's issuer URL, e.g. `https://id.example.com`. Discovery is read from
    /// `{issuer}/.well-known/openid-configuration`. Must be `https` (loopback `http` is
    /// allowed for development).
    pub issuer: String,
    pub client_id: String,
    /// Omit for a public client (PKCE is always used). Also settable with
    /// `CMS_OIDC_CLIENT_SECRET`.
    #[serde(default)]
    pub client_secret: Option<String>,
    /// Label on the login button.
    #[serde(default = "default_oidc_name")]
    pub name: String,
    /// Public base URL of this server, e.g. `https://cms.example.com`. The redirect URI
    /// registered with the provider is `{public_url}/api/auth/oidc/callback`.
    #[serde(default)]
    pub public_url: Option<String>,
    /// Full redirect URI, if it differs from the one derived from `public_url`.
    #[serde(default)]
    pub redirect_uri: Option<String>,
    #[serde(default = "default_oidc_scopes")]
    pub scopes: Vec<String>,
    /// Minimum assurance level (1 to 3). Sent as `acr_values` and checked on the ID token's
    /// `acr` (`urn:kith:acr:aalN`). With 2, a passkey sign-in with a TOTP step-up is needed
    /// where the identity only has weaker factors.
    #[serde(default)]
    pub min_aal: Option<u8>,
    /// Create a CMS account for a verified email that has none. Off by default: only
    /// people who already have an account can sign in.
    #[serde(default)]
    pub auto_create: bool,
    /// Roles for an account created by sign-in (and for a mapped sign-in that maps to nothing).
    #[serde(default)]
    pub default_roles: Vec<String>,
    /// Provider role or group name -> CMS role. When non-empty the provider is the source
    /// of truth: a user's CMS roles are replaced by the mapped ones on every sign-in. Needs
    /// the `groups` scope (the default scopes include it).
    #[serde(default)]
    pub role_map: std::collections::BTreeMap<String, String>,
    /// Where the browser goes after signing in.
    #[serde(default = "default_oidc_post_login")]
    pub post_login_url: String,
}

fn default_oidc_name() -> String {
    "Kith".to_string()
}
fn default_oidc_scopes() -> Vec<String> {
    ["openid", "email", "profile", "groups"]
        .iter()
        .map(|s| s.to_string())
        .collect()
}
fn default_oidc_post_login() -> String {
    "/admin/".to_string()
}

fn is_secure_or_loopback(u: &url::Url) -> bool {
    match u.scheme() {
        "https" => true,
        "http" => match u.host() {
            Some(url::Host::Domain(d)) => d == "localhost",
            Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
            Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
            None => false,
        },
        _ => false,
    }
}

impl OidcConfig {
    /// The redirect URI to register with the provider.
    pub fn redirect_uri(&self) -> Option<String> {
        self.redirect_uri.clone().or_else(|| {
            self.public_url
                .as_deref()
                .map(|u| format!("{}/api/auth/oidc/callback", u.trim_end_matches('/')))
        })
    }

    /// Refuse a configuration that could not work, or that would be unsafe, at startup.
    pub fn validate(&self) -> Result<(), String> {
        let issuer = url::Url::parse(&self.issuer).map_err(|e| format!("oidc.issuer: {e}"))?;
        if !is_secure_or_loopback(&issuer)
            || issuer.query().is_some()
            || issuer.fragment().is_some()
        {
            return Err(
                "oidc.issuer must be an https URL (http only for localhost) \
                        with no query or fragment"
                    .into(),
            );
        }
        if self.client_id.trim().is_empty() {
            return Err("oidc.client_id is required".into());
        }
        let redirect = self
            .redirect_uri()
            .ok_or("oidc: set public_url (or redirect_uri)")?;
        let redirect = url::Url::parse(&redirect).map_err(|e| format!("oidc.redirect_uri: {e}"))?;
        if !is_secure_or_loopback(&redirect) {
            return Err("oidc redirect URI must be https (http only for localhost)".into());
        }
        if !self.scopes.iter().any(|s| s == "openid") {
            return Err("oidc.scopes must include \"openid\"".into());
        }
        if let Some(n) = self.min_aal {
            if !(1..=3).contains(&n) {
                return Err("oidc.min_aal must be 1, 2 or 3".into());
            }
        }
        if !self.post_login_url.starts_with('/') || self.post_login_url.starts_with("//") {
            return Err(
                "oidc.post_login_url must be a path on this server, like \"/admin/\"".into(),
            );
        }
        Ok(())
    }
}
