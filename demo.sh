#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

FRONTEND_PID=""

cleanup() {
  echo ""
  echo "Stopping..."
  [[ -n "$FRONTEND_PID" ]] && kill "$FRONTEND_PID" 2>/dev/null || true
  docker compose --profile postgres down
}
trap cleanup EXIT INT TERM

# Throwaway local secrets (compose refuses to start without them). Export your own
# JWT_SECRET / POSTGRES_PASSWORD beforehand to keep data across demo runs.
export JWT_SECRET="${JWT_SECRET:-$(openssl rand -hex 32)}"
export POSTGRES_PASSWORD="${POSTGRES_PASSWORD:-$(openssl rand -hex 16)}"

echo "Starting CMS backend (PostgreSQL)..."
docker compose --profile postgres up -d --build

echo "Waiting for CMS to be healthy..."
until curl -sf http://localhost:8080/health >/dev/null; do
  sleep 1
done
echo "CMS is up."

echo ""
echo "First-run admin credentials (if this is a fresh database):"
docker compose --profile postgres logs cms-pg 2>/dev/null | grep -A4 "FIRST RUN — ADMIN CREATED" || true

echo "Starting frontend..."
cd "$SCRIPT_DIR/frontend"
npm install --silent
npm run dev &
FRONTEND_PID=$!

echo ""
echo "Demo running:"
echo "  Admin UI:   http://localhost:5173"
echo "  API docs:   http://localhost:8080/api/docs"
echo "  OpenAPI:    http://localhost:8080/api/openapi.json"
echo "  GraphQL:    http://localhost:8080/graphql"
echo "  REST v1:    http://localhost:8080/api/v1/<type>"
echo ""
echo "Press Ctrl+C to stop."
wait "$FRONTEND_PID"
