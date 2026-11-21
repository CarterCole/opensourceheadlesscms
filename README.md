# opensourceheadlesscms

A headless CMS server written in Rust, with a REST API, GraphQL API, and a React admin UI. Ships with SQLite for quick local use or PostgreSQL + Redis for a full stack.

## Quick start (demo)

```bash
./demo.sh
```

This brings up Postgres + Redis + the CMS backend via Docker Compose, then starts the frontend dev server.

- Admin UI: http://localhost:5173
- API docs: http://localhost:8080/api/docs
- OpenAPI: http://localhost:8080/api/openapi.json
- GraphQL: http://localhost:8080/graphql
- REST v1: http://localhost:8080/api/v1/<type>

### First-run admin login

On first startup against an empty database, the server auto-creates one admin
user and prints the generated credentials to its logs:

```
╔══════════════════════════════════════════════════╗
║            CMS FIRST RUN — ADMIN CREATED         ║
║  Email:    admin@localhost                       ║
║  Password: <randomly generated>                  ║
║  Change this password after first login!         ║
╚══════════════════════════════════════════════════╝
```

`demo.sh` prints this block automatically after the health check passes. If
you missed it, fetch it again with:

```bash
docker compose --profile postgres logs cms-pg | grep -A4 "FIRST RUN"
```

This only happens once — once any user exists in the database, the seed step
is a no-op. If you need to reset it, drop the `pgdata` volume:

```bash
docker compose --profile postgres down -v
```

## Running without Docker

### Backend (SQLite, no external DB)

```bash
cp cms.example.toml cms.toml   # env = "development": a random JWT secret is generated per run
cargo run --bin cms-server
```

### Backend (PostgreSQL)

```bash
docker compose --profile postgres up -d postgres redis
CMS_CONFIG_PATH=cms.postgres.toml cargo run --bin cms-server
```

### Frontend

```bash
cd frontend
npm install
npm run dev
```

## Docker Compose profiles

| Profile    | Command                                       | Stack                          |
|------------|------------------------------------------------|---------------------------------|
| `sqlite`   | `docker compose --profile sqlite up`           | CMS only, embedded SQLite       |
| `postgres` | `docker compose --profile postgres up`         | CMS + PostgreSQL + Redis        |

The `postgres` profile requires `JWT_SECRET` and `POSTGRES_PASSWORD` to be set (copy `.env.example` to `.env`, or export them; `demo.sh` generates throwaway values). `POSTGRES_DB` and `POSTGRES_USER` default to `cms`. Ports bind to `127.0.0.1` unless `CMS_BIND` says otherwise.

## Security

Read [SECURITY.md](SECURITY.md) before exposing an instance. In short: set a long random `CMS_JWT_SECRET` (the server refuses to start in `production` without one), keep `env = "production"`, put TLS and login rate-limiting in front of it, and rotate the first-run admin password. Report vulnerabilities privately via GitHub security advisories.

The admin UI authenticates with an `HttpOnly` session cookie (no token in page JavaScript); API clients use `Authorization: Bearer <token>` from `POST /api/auth/login`. Logout (`POST /api/auth/logout`) revokes the session server-side.

### Single sign-on ("Sign in with Kith")

The admin login page can also sign people in through an OpenID Connect provider such as a
[Kith](https://github.com/CarterCole/kith) hub (passkey login, `acr`-based step-up, groups and
roles). Register this server on the provider as a client, then add to `cms.toml`:

```toml
[oidc]
issuer = "https://id.example.com"          # discovery: {issuer}/.well-known/openid-configuration
client_id = "cms"                          # a public client (PKCE) unless you set CMS_OIDC_CLIENT_SECRET
public_url = "https://cms.example.com"     # redirect URI = {public_url}/api/auth/oidc/callback
# min_aal = 2                              # require a second factor (Kith: urn:kith:acr:aal2)
# auto_create = true                       # make an account for a verified email that has none
# default_roles = ["editor"]
# role_map = { "cms-admins" = "admin" }    # provider role/group -> CMS role, applied every sign-in
```

For a Kith hub, `clients.json` needs `{"client_id": "cms", "name": "CMS", "redirect_uris":
["https://cms.example.com/api/auth/oidc/callback"]}` (omit `client_secret` for a public client).
Users are matched by their **verified email**; see [SECURITY.md](SECURITY.md#authentication-model)
for what that trusts. The sign-in ends in the same session cookie as the password login, which
keeps working.

## Configuration

Server config lives in a TOML file (`cms.example.toml` / `cms.postgres.toml`), pointed to via `CMS_CONFIG_PATH`. Content types, workflows, and roles are defined under `content-types/`, `workflows/`, and `roles/` respectively.

## License

AGPL-3.0-only.
