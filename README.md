<div align="center">

# RFO Protocol

### The Application-Layer Network Protocol for AI Web-Scale Agents

**Version 1.0.0** · [Protocol Spec](PROTOCOL.md) · [Architecture](ARCHITECTURE.md) · [SDK Guide](SDK_GUIDE.md) · [Deployment](DEPLOYMENT.md)

[![CI](https://github.com/hmkhan10/rfo-protocol/actions/workflows/ci.yml/badge.svg)](https://github.com/hmkhan10/rfo-protocol/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-1.82+-orange.svg)](https://www.rust-lang.org)

</div>

---

## What is RFO?

RFO (**Research For Optimization**) is a next-generation application-layer network protocol purpose-built for AI agents that operate at web scale. It solves a fundamental problem: **how do AI agents efficiently discover, verify, and consume structured content from the internet?**

Today, AI agents scrape raw HTML, lose structural information, waste tokens on irrelevant content, and have no way to verify content authenticity. RFO replaces this chaos with a **cryptographically verified, token-optimized, structured content delivery protocol**.

```
┌─────────────────────────────────────────────────────────────────────┐
│                        THE RFO PROTOCOL                             │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│   AI Agent                          RFO Engine                     │
│      │                                   │                          │
│      │  ──── POST /rfo/handshake ──────▶ │  1. Verify nonce         │
│      │       { domain_url, nonce }       │  2. Fetch & parse        │
│      │                                   │  3. Extract coordinates  │
│      │                                   │  4. Generate site_id     │
│      │                                   │  5. Compile .doc/.mdoc   │
│      │                                   │  6. Score quality (0-100)│
│      │  ◀──── HandshakeResponse ──────── │  7. Cache + return       │
│      │       { header, payload }         │                          │
│      │                                   │                          │
│      │  Structured, verified, token-     │                          │
│      │  optimized content for the LLM    │                          │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

## Why RFO?

| Problem | Traditional Scraping | RFO Protocol |
|---------|---------------------|--------------|
| Content structure | Raw HTML, lost semantics | Structured `.doc` / `.mdoc` payloads |
| Token efficiency | Feed entire page to LLM | `< 1,500` token `.mdoc` with Q&A pairs |
| Content verification | None | HMAC-SHA256 verification signatures |
| Agent discovery | Hardcoded URLs | Cryptographic site IDs, quality scores |
| Real-time updates | Poll for changes | WebSocket pub/sub |
| Prompt injection | Vulnerable | 16-pattern injection sanitizer |
| Rate limiting | None | Per-IP + global DDoS protection |

## Key Features

- **Duplex Handshake** — Agent provides domain, engine compiles & returns structured payloads
- **Dual Payloads** — `.doc` (full knowledge) + `.mdoc` (token-optimized for LLMs)
- **Cryptographic Site IDs** — HMAC-SHA256, hourly rotation, replay protection
- **Quality Scoring** — Automated 0-100 score based on content structure & AEO readiness
- **Capability Negotiation** — JSON / MessagePack encoding, protocol versioning
- **WebSocket Pub/Sub** — Real-time domain update notifications
- **Binary Streaming** — Chunked transfer with checksums for large payloads
- **API Key Auth** — HMAC request signing, per-key permissions
- **DDoS Protection** — Per-IP rate limiting + global connection limits
- **Audit Logging** — Structured security event trail
- **Prompt Injection Defense** — 16-pattern sanitizer (EN + ZH)

## Quick Start

### 1. Run with Docker (recommended)

```bash
git clone https://github.com/hmkhan10/rfo-protocol.git
cd rfo-protocol

# Configure secrets
cp .env.example .env
# Edit .env with your secrets

# Start the stack
docker compose up -d

# Verify
curl http://localhost:3000/rfo/health
```

### 2. Run from source

```bash
# Prerequisites: Rust 1.82+, PostgreSQL 16

# Start PostgreSQL
docker run -d --name rfo-postgres \
  -e POSTGRES_USER=rfo \
  -e POSTGRES_PASSWORD=dev_pass \
  -e POSTGRES_DB=rfo_protocol \
  -p 5432:5432 postgres:16-alpine

# Set environment
export RFO_SECRET_KEY=$(openssl rand -hex 32)
export DATABASE_URL="postgres://rfo:dev_pass@localhost/rfo_protocol"

# Build and run
cargo build --release
./target/release/rfo-core
```

### 3. Try the handshake

```bash
# Generate a nonce
NONCE=$(uuidgen)
TIMESTAMP=$(date +%s)

# Handshake
curl -X POST http://localhost:3000/rfo/handshake \
  -H "Content-Type: application/json" \
  -H "X-API-Key: your-api-key" \
  -d "{
    \"domain_url\": \"https://example.com\",
    \"coordinates\": {},
    \"requested_payload\": \"Mdoc\",
    \"nonce\": \"$NONCE\",
    \"timestamp\": $TIMESTAMP
  }"
```

## Architecture

```
┌──────────────────────────────────────────────────────────────────────┐
│                        RFO CORE ENGINE                               │
│                                                                      │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────────────────┐│
│  │  Parser   │→│ Compiler │→│  Cache   │→│     HTTP Server       ││
│  │ HTML/MD   │  │ .doc     │  │ DashMap  │  │  Axum + Middleware   ││
│  │ →Parsed   │  │ .mdoc    │  │ TTL-based│  │  Rate Limit + Auth   ││
│  └──────────┘  │ Quality  │  └──────────┘  │  CORS + Audit        ││
│                │ Score    │                 └──────────────────────┘│
│  ┌──────────┐  └──────────┘  ┌──────────┐  ┌──────────────────────┐│
│  │  Crypto   │               │ Telemetry│  │    WebSocket         ││
│  │ HMAC-SHA256│              │ Metrics  │  │  WsManager pub/sub   ││
│  │ Site ID   │               │ Reports  │  │  Domain subscriptions││
│  └──────────┘               └──────────┘  └──────────────────────┘│
│                                                                      │
│  ┌──────────────────────────────────────────────────────────────────┐│
│  │                    PostgreSQL Database                           ││
│  │  sites │ handshake_logs │ audit_logs               ││
│  └──────────────────────────────────────────────────────────────────┘│
└──────────────────────────────────────────────────────────────────────┘
```

## API Endpoints

| Endpoint | Method | Auth | Description |
|----------|--------|------|-------------|
| `/rfo/health` | GET | No | Health check & protocol version |
| `/rfo/capabilities` | GET | No | Server capabilities & features |
| `/rfo/negotiate` | POST | No | Capability negotiation |
| `/rfo/handshake` | POST | Yes | Duplex handshake (compile domain) |
| `/rfo/batch-handshake` | POST | Yes | Batch handshake (up to 20 domains) |
| `/rfo/doc/:domain` | GET | Yes | Full `.doc` payload |
| `/rfo/mdoc/:domain` | GET | Yes | Mini `.mdoc` payload |
| `/rfo/stream/:domain` | GET | Yes | Stream `.doc` (binary) |
| `/rfo/stream-mdoc/:domain` | GET | Yes | Stream `.mdoc` (binary) |
| `/rfo/sites` | GET | Yes | List registered sites |
| `/rfo/telemetry` | GET | Yes | Telemetry dashboard |
| `/rfo/ws` | GET | No | WebSocket (real-time updates) |

Full API spec: [`openapi.yaml`](openapi.yaml)

## Project Structure

```
rfo-protocol/
├── src/
│   ├── main.rs              # Entry point (server + CLI)
│   ├── lib.rs               # Library exports
│   ├── rfo_protocol.rs      # Core types (Handshake, Payload, Header)
│   ├── protocol.rs          # Version negotiation, streaming, WebSocket types
│   ├── parser.rs            # HTML/Markdown parser + injection sanitizer
│   ├── compiler.rs          # Content → .doc/.mdoc compiler + quality scoring
│   ├── crypto/site_id.rs    # HMAC-SHA256 site ID generation
│   ├── cache/mod.rs         # DashMap concurrent cache with TTL
│   ├── auth.rs              # API key store, HMAC signing, middleware
│   ├── audit.rs             # Audit logger, DDoS protection, CORS
│   ├── telemetry.rs         # Metrics, quality trends, reports
│   ├── client.rs            # Rust client SDK
│   ├── cli.rs               # CLI (compile, watch, serve, inspect, audit)
│   └── server/
│       ├── mod.rs
│       ├── handlers.rs      # HTTP route handlers
│       ├── middleware.rs     # Rate limiting, IP extraction
│       └── websocket.rs     # WebSocket pub/sub manager
├── tests/
│   ├── integration.rs       # Full HTTP stack tests (16)
│   ├── security.rs          # Security tests (45)
│   ├── concurrency.rs       # Race condition tests (11)
│   └── protocol.rs          # Protocol compliance tests (20)
├── migrations/
│   ├── 001_initial.sql      # sites, handshake_logs
│   └── 002_audit_logs.sql   # audit_logs
├── .github/workflows/
│   ├── ci.yml               # Lint → Test → Build → Security Audit
│   └── release.yml          # Build → Docker → GitHub Release
├── Dockerfile               # Multi-stage, non-root, Alpine
├── docker-compose.yml       # PostgreSQL + Engine
├── openapi.yaml             # OpenAPI 3.1 spec (all endpoints)
├── .env.example             # Documented environment variables
└── Cargo.toml
```

## Documentation

| Document | Description |
|----------|-------------|
| [**PROTOCOL.md**](PROTOCOL.md) | Wire format, handshake protocol, payload schemas, WebSocket spec |
| [**ARCHITECTURE.md**](ARCHITECTURE.md) | Component diagrams, data flow, security model, design decisions |
| [**SDK_GUIDE.md**](SDK_GUIDE.md) | Rust client SDK: installation, quick start, advanced usage |
| [**DEPLOYMENT.md**](DEPLOYMENT.md) | Docker, environment variables, security hardening, production checklist |
| [**CONTRIBUTING.md**](CONTRIBUTING.md) | How to contribute, code style, testing, PR process |
| [**openapi.yaml**](openapi.yaml) | OpenAPI 3.1 spec for generating client SDKs in any language |

## Testing

```bash
# Run all 145 tests
cargo test

# Run specific suite
cargo test --test security      # 45 security tests
cargo test --test integration   # 16 integration tests
cargo test --test concurrency   # 11 concurrency tests
cargo test --test protocol      # 20 protocol compliance tests
```

## Deployment

```bash
# Development
docker compose up -d

# Production
cp .env.example .env
# Set strong secrets in .env
docker compose -f docker-compose.yml up -d

# Verify
curl -f http://localhost:3000/rfo/health
```

See [DEPLOYMENT.md](DEPLOYMENT.md) for the full production checklist.

## Security

- **API Key Authentication** — `X-API-Key` header on all protected endpoints
- **HMAC Request Signing** — SHA-256 body integrity verification
- **Nonce Replay Protection** — 5-minute freshness window
- **DDoS Protection** — Per-IP (100/min) + global (1000) connection limits
- **Prompt Injection Defense** — 16-pattern sanitizer (EN + ZH)
- **Audit Logging** — All security events logged to PostgreSQL
- **CORS** — Configurable allowed origins
- **Read-Only Container** — Docker runs as non-root, read-only filesystem

Report security issues responsibly. See [CONTRIBUTING.md](CONTRIBUTING.md).

## Roadmap

- [x] Phase 1: Core Engine Foundation
- [x] Phase 2: PostgreSQL Live Wiring
- [x] Phase 3: Client SDK
- [x] Phase 4: Advanced Cache & Telemetry
- [x] Phase 5: Content Pipeline CLI
- [x] Phase 6: Security Hardening
- [ ] Phase 7: Content pipeline extensions
- [x] Phase 8: Protocol Extensions (streaming, WebSocket, capability negotiation)
- [x] Phase 9: Production Deployment (Docker, CI/CD, OpenAPI)
- [x] Phase 10: Testing & Hardening (145 tests, 2 bugs fixed)
- [x] Phase 11: Documentation & Architecture
- [ ] Phase 12: Performance Optimization (benchmarks, profiling)

## License

MIT License. See [LICENSE](LICENSE).

---

<div align="center">

**Built for the AI agents that will shape the future of the web.**

</div>
