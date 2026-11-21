//! In-process request rate limiting and login brute-force throttling.
//!
//! State lives in memory, so limits are per server process. That is fine for a single
//! instance; for a horizontally scaled deployment also enforce limits at the proxy.

use crate::config::{RateLimitConfig, SecurityConfig};
use axum::{
    extract::{ConnectInfo, Request, State},
    http::{header, HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

/// Client address as seen by the server (inserted into request extensions by
/// [`rate_limit_middleware`]).
#[derive(Debug, Clone)]
pub struct ClientIp(pub String);

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Class {
    Public,
    Graphql,
    Auth,
}

struct Bucket {
    tokens: f64,
    last: Instant,
}

struct Inner {
    cfg: RateLimitConfig,
    trust_proxy_headers: bool,
    buckets: Mutex<HashMap<(Class, String), Bucket>>,
}

/// Token-bucket limiter keyed by (route class, client address).
#[derive(Clone)]
pub struct RateLimitState {
    inner: Arc<Inner>,
}

const MAX_TRACKED: usize = 50_000;

impl RateLimitState {
    pub fn new(cfg: RateLimitConfig, security: &SecurityConfig) -> Self {
        Self {
            inner: Arc::new(Inner {
                cfg,
                trust_proxy_headers: security.trust_proxy_headers,
                buckets: Mutex::new(HashMap::new()),
            }),
        }
    }

    fn rps(&self, class: Class) -> u32 {
        match class {
            Class::Public => self.inner.cfg.public_rps,
            Class::Graphql => self.inner.cfg.graphql_rps,
            Class::Auth => self.inner.cfg.auth_rps,
        }
    }

    /// Returns `Err(retry_after_seconds)` when the caller is over its limit.
    fn take(&self, class: Class, ip: &str) -> Result<(), u64> {
        let rps = self.rps(class);
        if rps == 0 {
            return Ok(()); // 0 disables the limit for this class
        }
        let rate = rps as f64;
        let capacity = (rate * 2.0).max(1.0); // allow short bursts of 2 seconds' worth
        let now = Instant::now();

        let mut buckets = self.inner.buckets.lock().unwrap_or_else(|e| e.into_inner());
        if buckets.len() > MAX_TRACKED {
            buckets.retain(|_, b| now.duration_since(b.last) < Duration::from_secs(300));
        }
        let bucket = buckets.entry((class, ip.to_string())).or_insert(Bucket {
            tokens: capacity,
            last: now,
        });
        let elapsed = now.duration_since(bucket.last).as_secs_f64();
        bucket.tokens = (bucket.tokens + elapsed * rate).min(capacity);
        bucket.last = now;
        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            Ok(())
        } else {
            Err(((1.0 - bucket.tokens) / rate).ceil().max(1.0) as u64)
        }
    }

    fn client_ip(&self, req: &Request) -> String {
        if self.inner.trust_proxy_headers {
            let headers = req.headers();
            let forwarded = headers
                .get("x-forwarded-for")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.split(',').next())
                .map(str::trim)
                .filter(|s| !s.is_empty());
            let real = headers
                .get("x-real-ip")
                .and_then(|v| v.to_str().ok())
                .map(str::trim)
                .filter(|s| !s.is_empty());
            if let Some(ip) = forwarded.or(real) {
                return ip.to_string();
            }
        }
        req.extensions()
            .get::<ConnectInfo<SocketAddr>>()
            .map(|c| c.0.ip().to_string())
            .unwrap_or_else(|| "unknown".to_string())
    }
}

fn too_many(retry_after: u64) -> Response {
    let mut resp = (
        StatusCode::TOO_MANY_REQUESTS,
        axum::Json(serde_json::json!({"error": "rate limit exceeded"})),
    )
        .into_response();
    if let Ok(v) = HeaderValue::from_str(&retry_after.to_string()) {
        resp.headers_mut().insert(header::RETRY_AFTER, v);
    }
    resp
}

/// Per-client-address rate limiting; also records the client address for handlers.
pub async fn rate_limit_middleware(
    State(state): State<RateLimitState>,
    mut req: Request,
    next: Next,
) -> Response {
    let ip = state.client_ip(&req);
    let path = req.uri().path();
    let class = if path == "/health" {
        None
    } else if path.starts_with("/api/auth/") {
        Some(Class::Auth)
    } else if path.starts_with("/graphql") {
        Some(Class::Graphql)
    } else {
        Some(Class::Public)
    };
    if let Some(class) = class {
        if let Err(retry_after) = state.take(class, &ip) {
            return too_many(retry_after);
        }
    }
    req.extensions_mut().insert(ClientIp(ip));
    next.run(req).await
}

// ------------------------------------------------------------------------------------
// Login throttling
// ------------------------------------------------------------------------------------

struct Failures {
    count: u32,
    window_start: Instant,
}

/// Counts failed logins per client address and per account, and locks out whichever
/// crosses its threshold for the rest of the window.
///
/// The per-account limit stops distributed guessing against one account, at the cost that
/// an attacker can keep an account locked by failing on purpose; it is set high enough
/// (default 20) that ordinary typos never trigger it.
pub struct LoginThrottle {
    max_per_ip: u32,
    max_per_account: u32,
    window: Duration,
    state: Mutex<(HashMap<String, Failures>, HashMap<String, Failures>)>,
}

impl LoginThrottle {
    pub fn new(security: &SecurityConfig) -> Self {
        Self {
            max_per_ip: security.login_max_failures,
            max_per_account: security.login_account_max_failures,
            window: Duration::from_secs(security.login_lockout_seconds),
            state: Mutex::new((HashMap::new(), HashMap::new())),
        }
    }

    fn normalise(email: &str) -> String {
        email.trim().to_lowercase()
    }

    fn locked(
        map: &HashMap<String, Failures>,
        key: &str,
        max: u32,
        window: Duration,
    ) -> Option<u64> {
        if max == 0 {
            return None; // 0 disables
        }
        let f = map.get(key)?;
        let age = Instant::now().duration_since(f.window_start);
        if age < window && f.count >= max {
            Some((window - age).as_secs().max(1))
        } else {
            None
        }
    }

    /// `Err(retry_after_seconds)` if this address or account is currently locked out.
    pub fn check(&self, ip: &str, email: &str) -> Result<(), u64> {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(s) = Self::locked(&state.0, ip, self.max_per_ip, self.window) {
            return Err(s);
        }
        if let Some(s) = Self::locked(
            &state.1,
            &Self::normalise(email),
            self.max_per_account,
            self.window,
        ) {
            return Err(s);
        }
        Ok(())
    }

    pub fn record_failure(&self, ip: &str, email: &str) {
        let now = Instant::now();
        let mut guard = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let (by_ip, by_account) = &mut *guard;
        for (map, key) in [
            (by_ip, ip.to_string()),
            (by_account, Self::normalise(email)),
        ] {
            if map.len() > MAX_TRACKED {
                map.retain(|_, f| now.duration_since(f.window_start) < self.window);
            }
            let entry = map.entry(key).or_insert(Failures {
                count: 0,
                window_start: now,
            });
            if now.duration_since(entry.window_start) >= self.window {
                entry.count = 0;
                entry.window_start = now;
            }
            entry.count += 1;
        }
    }

    pub fn record_success(&self, ip: &str, email: &str) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.0.remove(ip);
        state.1.remove(&Self::normalise(email));
    }
}

pub fn retry_response(retry_after: u64) -> Response {
    too_many(retry_after)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn security(max_ip: u32, max_acct: u32) -> SecurityConfig {
        SecurityConfig {
            login_max_failures: max_ip,
            login_account_max_failures: max_acct,
            login_lockout_seconds: 60,
            ..Default::default()
        }
    }

    #[test]
    fn bucket_allows_burst_then_blocks() {
        let cfg = RateLimitConfig {
            public_rps: 2,
            graphql_rps: 2,
            auth_rps: 2,
        };
        let rl = RateLimitState::new(cfg, &SecurityConfig::default());
        // capacity is 2 * rps = 4
        for _ in 0..4 {
            assert!(rl.take(Class::Public, "1.1.1.1").is_ok());
        }
        assert!(rl.take(Class::Public, "1.1.1.1").is_err());
        // other addresses and classes are unaffected
        assert!(rl.take(Class::Public, "2.2.2.2").is_ok());
        assert!(rl.take(Class::Auth, "1.1.1.1").is_ok());
    }

    #[test]
    fn zero_rps_disables_limit() {
        let cfg = RateLimitConfig {
            public_rps: 0,
            graphql_rps: 0,
            auth_rps: 0,
        };
        let rl = RateLimitState::new(cfg, &SecurityConfig::default());
        for _ in 0..1000 {
            assert!(rl.take(Class::Public, "x").is_ok());
        }
    }

    #[test]
    fn login_locks_ip_after_max_failures_and_success_resets() {
        let t = LoginThrottle::new(&security(3, 100));
        for _ in 0..3 {
            assert!(t.check("9.9.9.9", "a@x.com").is_ok());
            t.record_failure("9.9.9.9", "a@x.com");
        }
        assert!(t.check("9.9.9.9", "a@x.com").is_err());
        assert!(
            t.check("8.8.8.8", "a@x.com").is_ok(),
            "other address unaffected"
        );
        t.record_success("9.9.9.9", "a@x.com");
        assert!(t.check("9.9.9.9", "a@x.com").is_ok());
    }

    #[test]
    fn login_locks_account_across_addresses_case_insensitively() {
        let t = LoginThrottle::new(&security(100, 3));
        for i in 0..3 {
            t.record_failure(&format!("10.0.0.{i}"), "Admin@X.com");
        }
        assert!(t.check("10.0.0.99", "admin@x.com").is_err());
        assert!(t.check("10.0.0.99", "other@x.com").is_ok());
    }
}
