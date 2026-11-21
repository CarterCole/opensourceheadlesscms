# Security Policy

## Reporting a vulnerability

Please **do not open a public issue** for security problems.

Report privately through GitHub: **Security → Report a vulnerability** on this
repository (private vulnerability reporting / security advisories). Include the affected
version or commit, reproduction steps, and the impact you observed.

You can expect an acknowledgement within a few days. Fixes are developed privately and
disclosed in a published advisory once a patched release is available.

## Supported versions

This project is pre-1.0. Only the latest release (and `main`) receives security fixes.

## Deploying safely

The defaults are tuned for local development. Before exposing an instance:

1. **Set a real `jwt_secret`** — `CMS_JWT_SECRET=$(openssl rand -hex 32)`. Anyone who
   knows the secret can mint a session token. With `env = "production"` (the default) the
   server refuses to start if the secret is missing, shorter than 32 characters, or the
   placeholder from the example configs. With `env = "development"` it generates a random
   per-process secret instead.
2. **Keep `env = "production"`.** Development mode serves the GraphQL playground and
   issues cookies without the `Secure` flag.
3. **Terminate TLS in front of the server** (nginx, Caddy, a cloud load balancer). The
   server speaks plain HTTP; the session cookie is `Secure` outside development, so the
   admin UI only works over HTTPS (or `localhost`).
4. **If you run behind a reverse proxy, set `security.trust_proxy_headers = true`** *and*
   make sure the proxy overwrites `X-Forwarded-For`. Otherwise every client appears to
   come from the proxy's address and shares one rate-limit/lockout bucket. Leave it
   `false` when clients connect directly (clients could otherwise spoof the header).
5. **Change the first-run admin password.** On an empty database the server creates
   `admin@localhost` with a random password printed once to stdout. Treat container logs
   as sensitive until you've rotated it.
6. **Use strong database credentials** and don't publish the database port.
   `docker-compose.yml` requires `JWT_SECRET` and `POSTGRES_PASSWORD` and binds to
   `127.0.0.1` by default (`CMS_BIND` overrides).
7. **Keep the image current** — the published image is rebuilt on each release tag.

## Authentication model

- **Browser admin UI:** logging in sets an `HttpOnly; SameSite=Strict` session cookie
  (`Secure` outside development). The token is never exposed to page JavaScript, so an XSS
  bug cannot exfiltrate it. Cookie-authenticated requests must come from the server's own
  origin (or one listed in `cors_allowed_origins`); a request carrying a foreign `Origin`
  is treated as unauthenticated. This also blocks cross-site WebSocket hijacking.
- **API clients:** `POST /api/auth/login` also returns the token; send it as
  `Authorization: Bearer <token>`.
- **Sessions are revocable.** Every request re-checks the user in the database: the user
  must still exist, the token's *session epoch* must match the user's current epoch, and
  roles are read from the database, never from the token. `POST /api/auth/logout` bumps
  the epoch (invalidating every token the user holds); deleting a user or changing their
  roles takes effect on the next request. Token lifetime is `security.jwt_ttl_seconds`
  (default 8 hours).
- **Single sign-on (optional):** with an `[oidc]` section the login page also offers
  "Sign in with Kith" (any OpenID Connect provider works). The flow is authorization code
  with PKCE; `state`, `nonce` and the PKCE verifier sit in a short-lived signed `HttpOnly`
  cookie, so a callback only completes in the browser that started it. The ID token must be
  RS256, signed by a key in the issuer's JWKS, with the configured `iss`, `aud` and `nonce`
  (and `acr` when `min_aal` is set); an `iss` response parameter that names another provider
  is refused (RFC 9207). The user is found by a **verified** email only. Without
  `auto_create` there is no account creation: an unknown email is refused. Accounts made by
  SSO have no usable password. Treat the provider as fully trusted for identity: whoever it
  says owns an address can sign in as that account, and with `role_map` it also decides roles
  (a sign-in that maps to no role gets `default_roles`, so mind that when an administrator
  signs in through it). The end result is the same session cookie and epoch as a password login.
- **Brute-force protection:** per-address token-bucket rate limits (`[rate_limit]`:
  `public_rps`, `graphql_rps`, `auth_rps`; 0 disables a class) plus login lockouts —
  5 failed logins per address and 20 per account within 15 minutes (`security.login_*`).
  The per-account limit means someone can deliberately keep one account locked for a
  window; it is set high so ordinary typos never trigger it.

## Access model (what the APIs enforce)

| Surface | Who | Notes |
|---|---|---|
| `GET /api/v1/*` (REST) | Anyone | `published` content only. Fields restricted by any role or by a field's `access:` rule are stripped. Signed-in users with read permission on a type can also see other statuses (those responses are `private, no-store`). |
| `POST /graphql`, `/graphql/ws` | Signed-in users | Requires a valid session, including for introspection. Per-type RBAC and per-field read/write rules apply; subscription events are redacted per subscriber. Query depth is limited. |
| `/api/auth/me`, `/api/auth/logout`, `/api/admin/config` | Signed-in users | |
| `/api/docs`, `/api/openapi.json`, `/health` | Anyone | |

Workflow-governed content types cannot have `status` set through `createContent` /
`updateContent`; status changes go through `transitionContent`, which checks the item's
real current state against the workflow's role rules.

Writes are validated against the content type definition: unknown fields, wrong value
types, empty required fields and oversized text are rejected. Editing an item never erases
fields the editor isn't allowed to see. `created_by` is the permanent original author;
`updated_by` is the last editor.

Other hardening: responses carry `X-Content-Type-Options`, `X-Frame-Options: DENY` and
`Referrer-Policy: no-referrer`; the admin UI is served with a strict
`Content-Security-Policy`; request bodies are capped at 1 MiB; CORS is same-origin only
unless `security.cors_allowed_origins` lists origins (credentials are never allowed
cross-origin); full-text search input is turned into quoted literals before it reaches
SQLite's query parser.

## Known limitations

- **Rate limiting is per process** (in memory). A horizontally scaled deployment should
  also limit at the proxy / WAF.
- **Open WebSocket connections are authenticated once**, at `connection_init`. A logout or
  revocation stops new connections and requests immediately, but a socket that is already
  open keeps receiving events until it closes.
- **Every authenticated request costs one small database lookup** (the revocation check).
- **Existing `created_by` values** from before the `002_security` migration held the last
  editor, not the author; that history can't be recovered. The migration copies them into
  `updated_by` so nothing is lost.
- **Dependency advisory `RUSTSEC-2026-0459`** (bundled SQLite via `libsqlite3-sys`,
  medium): the patched crate is not yet reachable because the latest `sqlx` still pins the
  old version. SQLite backend only; mitigated as described in `.cargo/audit.toml`.
- Personal API tokens: the `_cms_api_tokens` table and store methods exist but are not
  wired into authentication yet.
- The `D1` database driver and gRPC are not implemented.

## Automated checks

CI runs `cargo audit` and `npm audit` (production dependencies, high and above) and builds
the Docker image on every push and pull request, and Dependabot opens update PRs weekly.
Advisory exceptions, with reasons, are listed in `.cargo/audit.toml`.
