# ─────────────────────────────────────────────────────────────────────────────
# RFO Protocol Engine — Production Multi-Stage Dockerfile
# ─────────────────────────────────────────────────────────────────────────────
# Architecture decisions:
#   - cargo-chef: dependency layer caching (rebuild only src on code change)
#   - Alpine: minimal base (~5MB vs ~80MB for debian)
#   - Non-root user: process drops privileges
#   - Migrations embedded: sqlx::migrate!() at compile time, zero runtime deps
#   - Read-only: no write access to filesystem except /tmp
# ─────────────────────────────────────────────────────────────────────────────

# ── Stage 1: Chef (planner) ─────────────────────────────────────────────────
# Pre-builds dependency recipe so cargo only re-compiles OUR code on changes.
FROM rust:1.82-alpine AS chef
RUN apk add --no-cache musl-dev openssl-dev pkgconfig
RUN cargo install cargo-chef --locked
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# ── Stage 2: Builder (compile) ──────────────────────────────────────────────
FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json

# Build dependencies only (cached unless Cargo.toml changes)
RUN cargo chef cook --release --recipe-path recipe.json

# Copy source and build the actual binary
COPY migrations migrations
COPY src src
COPY Cargo.toml Cargo.toml
RUN cargo build --release --bin rfo-core

# ── Stage 3: Runtime (production) ───────────────────────────────────────────
# Alpine minimal — only what we need to run the binary.
FROM alpine:3.20 AS runtime

RUN apk add --no-cache \
    ca-certificates \
    openssl \
    curl \
    tzdata \
    && rm -rf /var/cache/apk/*

# Create non-root user (security: never run as root)
RUN addgroup -g 1000 rfo \
    && adduser -u 1000 -G rfo -s /bin/sh -D rfo

WORKDIR /app

# Copy the compiled binary
COPY --from=builder /app/target/release/rfo-core /app/rfo-core

# Copy migrations for runtime migration runner
COPY --from=builder /app/migrations /app/migrations

# Ownership
RUN chown -R rfo:rfo /app

# Switch to non-root user
USER rfo

# Environment defaults (override in docker-compose or at runtime)
ENV RUST_LOG=info \
    RFO_BIND_ADDR=0.0.0.0:3000 \
    DATABASE_URL=postgres://rfo:rfo_secret@localhost:5432/rfo_protocol

# Expose the engine port
EXPOSE 3000

# Health check — hits the public /rfo/health endpoint
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -f http://localhost:3000/rfo/health || exit 1

# Signal handling: Rust binary handles SIGTERM via axum's graceful shutdown
STOPSIGNAL SIGTERM

ENTRYPOINT ["/app/rfo-core"]
