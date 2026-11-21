# Stage 1: build
FROM rust:1.100-slim AS builder

RUN apt-get update && apt-get install -y pkg-config libssl-dev curl && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Cache dependency build separately from source changes
COPY Cargo.toml Cargo.lock ./

# Stub src so cargo can fetch and build deps
RUN mkdir -p src && \
    echo "fn main() {}" > src/main.rs && \
    echo "// stub" > src/lib.rs

RUN cargo build --release --bin cms-server 2>&1 || true

# Copy real source and rebuild
COPY src/ src/
COPY migrations/ migrations/

RUN touch src/*.rs src/**/*.rs 2>/dev/null || true
RUN cargo build --release --bin cms-server

# Stage 2: minimal runtime image
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates libssl3 curl && rm -rf /var/lib/apt/lists/*

# Run as an unprivileged user
RUN useradd --system --uid 10001 --home-dir /app --shell /usr/sbin/nologin cms \
    && mkdir -p /app/data /app/content /app/media \
    && chown -R cms:cms /app

WORKDIR /app

COPY --from=builder /app/target/release/cms-server /usr/local/bin/cms-server
COPY migrations/ migrations/
COPY content-types/ content-types/
COPY workflows/ workflows/
COPY roles/ roles/
COPY globals/ globals/

USER cms

# Default config location; mount or override via CMS_CONFIG_PATH
ENV CMS_CONFIG_PATH=/app/cms.toml
ENV RUST_LOG=cms=info

EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s \
  CMD curl -f http://localhost:8080/health || exit 1

ENTRYPOINT ["cms-server"]
