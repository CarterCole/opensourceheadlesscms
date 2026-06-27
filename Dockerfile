# Stage 1: build
FROM rust:1.82-slim AS builder

# Install protobuf compiler (needed by cms-proto tonic-build)
RUN apt-get update && apt-get install -y protobuf-compiler pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Cache dependency build separately from source changes
COPY Cargo.toml Cargo.lock ./
COPY crates/cms-core/Cargo.toml crates/cms-core/
COPY crates/cms-config/Cargo.toml crates/cms-config/
COPY crates/cms-db/Cargo.toml crates/cms-db/
COPY crates/cms-cache/Cargo.toml crates/cms-cache/
COPY crates/cms-rbac/Cargo.toml crates/cms-rbac/
COPY crates/cms-schema/Cargo.toml crates/cms-schema/
COPY crates/cms-scripting/Cargo.toml crates/cms-scripting/
COPY crates/cms-flatfile/Cargo.toml crates/cms-flatfile/
COPY crates/cms-http/Cargo.toml crates/cms-http/
COPY crates/cms-proto/Cargo.toml crates/cms-proto/
COPY crates/cms-grpc/Cargo.toml crates/cms-grpc/
COPY crates/cms-server/Cargo.toml crates/cms-server/

# Stub all lib.rs / main.rs so cargo can fetch and build deps
RUN for crate in cms-core cms-config cms-db cms-cache cms-rbac cms-schema cms-scripting cms-flatfile cms-http cms-proto cms-grpc; do \
      mkdir -p crates/$crate/src && echo "// stub" > crates/$crate/src/lib.rs; \
    done && \
    mkdir -p crates/cms-proto && echo "fn main() {}" > crates/cms-proto/build.rs && \
    mkdir -p crates/cms-server/src && echo "fn main() {}" > crates/cms-server/src/main.rs

RUN cargo build --release -p cms-server 2>&1 || true

# Now copy real source and rebuild (only changed crates recompile)
COPY crates/ crates/
COPY proto/ proto/
COPY migrations/ migrations/

RUN touch crates/*/src/*.rs crates/*/src/**/*.rs 2>/dev/null || true
RUN cargo build --release -p cms-server

# Stage 2: minimal runtime image
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates libssl3 && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /app/target/release/cms-server /usr/local/bin/cms-server
COPY migrations/ migrations/
COPY content-types/ content-types/
COPY workflows/ workflows/
COPY roles/ roles/

# Default config location; mount or override via CMS_CONFIG_PATH
ENV CMS_CONFIG_PATH=/app/cms.toml
ENV RUST_LOG=cms=info

EXPOSE 8080 50051

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s \
  CMD curl -f http://localhost:8080/health || exit 1

ENTRYPOINT ["cms-server"]
