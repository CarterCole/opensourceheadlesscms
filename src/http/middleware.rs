use crate::schema::SchemaContext;
use axum::{
    extract::{Request, State},
    http::{
        header::{AUTHORIZATION, COOKIE, HOST, ORIGIN},
        HeaderMap,
    },
    middleware::Next,
    response::Response,
};

use crate::http::auth::{authenticate_token, SESSION_COOKIE};

#[derive(Clone)]
pub struct AuthMiddlewareState {
    pub jwt_secret: String,
    pub ctx_template: SchemaContext,
    /// Extra origins (from `cors_allowed_origins`) trusted to send cookie-authenticated
    /// requests in addition to the server's own origin.
    pub trusted_origins: Vec<String>,
}

/// Pull the session cookie out of the request headers.
fn cookie_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(name, _)| *name == SESSION_COOKIE)
        .map(|(_, value)| value.to_string())
        .filter(|v| !v.is_empty())
}

/// Cookies are sent automatically by browsers, so a cookie-authenticated request must come
/// from our own origin (or an explicitly trusted one). `SameSite=Strict` already blocks
/// cross-site sends; this is defence in depth, and also covers cross-site WebSocket
/// hijacking. Requests with no `Origin` header (same-origin GETs, non-browser clients) pass.
fn origin_allowed(headers: &HeaderMap, trusted: &[String]) -> bool {
    let Some(origin) = headers.get(ORIGIN).and_then(|v| v.to_str().ok()) else {
        return !headers.contains_key(ORIGIN);
    };
    if trusted.iter().any(|t| t == origin) {
        return true;
    }
    let origin_host = origin
        .strip_prefix("https://")
        .or_else(|| origin.strip_prefix("http://"));
    let host = headers.get(HOST).and_then(|v| v.to_str().ok());
    matches!((origin_host, host), (Some(o), Some(h)) if o.eq_ignore_ascii_case(h))
}

/// Session auth middleware — attaches a `SchemaContext` carrying the caller's identity to
/// the request. Credentials: `Authorization: Bearer <jwt>` (API clients) or the HttpOnly
/// session cookie (browser admin UI). Public routes still proceed with no user attached.
pub async fn jwt_auth_middleware(
    State(state): State<AuthMiddlewareState>,
    mut request: Request,
    next: Next,
) -> Response {
    let mut ctx = state.ctx_template.clone();

    let bearer = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::to_string);

    let token = match bearer {
        Some(t) => Some(t),
        None if origin_allowed(request.headers(), &state.trusted_origins) => {
            cookie_token(request.headers())
        }
        None => None,
    };

    if let Some(token) = token {
        if let Some((user_id, roles)) =
            authenticate_token(state.ctx_template.store.as_ref(), &state.jwt_secret, &token).await
        {
            ctx = ctx.with_user(user_id, roles);
        }
    }

    request.extensions_mut().insert(ctx);
    next.run(request).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn headers(pairs: &[(&'static str, &'static str)]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for (k, v) in pairs {
            h.insert(*k, HeaderValue::from_static(v));
        }
        h
    }

    #[test]
    fn parses_session_cookie_among_others() {
        let h = headers(&[("cookie", "a=1; cms_session=tok.en; b=2")]);
        assert_eq!(cookie_token(&h).as_deref(), Some("tok.en"));
        assert_eq!(cookie_token(&headers(&[("cookie", "a=1")])), None);
        assert_eq!(cookie_token(&headers(&[("cookie", "cms_session=")])), None);
    }

    #[test]
    fn origin_must_match_host_or_be_trusted() {
        let same = headers(&[
            ("origin", "https://cms.example.com"),
            ("host", "cms.example.com"),
        ]);
        assert!(origin_allowed(&same, &[]));

        let cross = headers(&[
            ("origin", "https://evil.example"),
            ("host", "cms.example.com"),
        ]);
        assert!(!origin_allowed(&cross, &[]));
        assert!(origin_allowed(
            &cross,
            &["https://evil.example".to_string()]
        ));

        let null = headers(&[("origin", "null"), ("host", "cms.example.com")]);
        assert!(!origin_allowed(&null, &[]));

        let none = headers(&[("host", "cms.example.com")]);
        assert!(origin_allowed(&none, &[]));
    }
}
