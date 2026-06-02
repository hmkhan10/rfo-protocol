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
┌──────────────────────────────────────────────────────────────────────────┐
│                          THE RFO PROTOCOL                                │
├──────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│   AI Agent                              RFO Engine                       │
│      │                                       │                           │
│      │  ──── POST /rfo/handshake ──────────▶ │  1. Verify nonce          │
│      │       { domain_url, nonce }           │  2. Fetch & parse         │
│      │                                       │  3. Extract coordinates   │
│      │                                       │  4. Generate site_id      │
│      │                                       │  5. Compile .doc/.mdoc    │
│      │                                       │  6. Score quality (0-100) │
│      │  ◀──── HandshakeResponse ──────────── │  7. Cache + return        │
│      │       { header, payload }             │                           │
│      │                                       │                           │
│      │  Structured, verified, token-         │                           │
│      │  optimized content for the LLM        │                           │
│      │                                       │                           │
│      │  ──── GET /rfo/doc/{domain} ────────▶ │  Return full .doc         │
│      │  ◀──── FullDocPayload ────────────── │  (deep knowledge)         │
│      │                                       │                           │
│      │  ──── GET /rfo/mdoc/{domain} ───────▶ │  Return mini .mdoc        │
│      │  ◀──── MiniDocPayload ────────────── │  (token-optimized)        │
│      │                                       │                           │
│      │  ──── GET /rfo/ws ──────────────────▶ │  WebSocket subscription   │
│      │  ◀──── Real-time updates ──────────── │  (live domain changes)    │
│      │                                       │                           │
└──────────────────────────────────────────────────────────────────────────┘
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
| Domain support | Standard TLDs only | Native `.opt` AI-optimized domains |

## Key Features

- **Duplex Handshake** — Agent provides domain, engine compiles & returns structured payloads
- **Dual Payloads** — `.doc` (full knowledge) + `.mdoc` (token-optimized for LLMs)
- **Cryptographic Site IDs** — HMAC-SHA256, hourly rotation, replay protection
- **Quality Scoring** — Automated 0-100 score based on content structure & AEO readiness
- **Capability Negotiation** — JSON / MessagePack encoding, protocol versioning
- **WebSocket Pub/Sub** — Real-time domain update notifications
- **Binary Streaming** — Chunked transfer with CRC32 checksums for large payloads
- **.opt Domain Support** — Native AI-optimized TLD with SEO/GEO/AEO metadata
- **Document Pipeline** — Automatic .doc/.mdoc generation for websites
- **Admin API** — Full management interface with RBAC
- **Advanced Cryptography** — HMAC-SHA256/SHA512, HKDF key derivation, content integrity
- **API Key Auth** — HMAC request signing, per-key permissions
- **DDoS Protection** — Per-IP rate limiting + global connection limits
- **Audit Logging** — Structured security event trail
- **Prompt Injection Defense** — 16-pattern sanitizer (EN + ZH)

---

## Key Concepts (Glossary)

Understanding RFO requires understanding its core building blocks:

### `.doc` — Full Knowledge Document

A `.doc` is the **complete, deep-knowledge payload** for a web page. Think of it as the full textbook version of a page — every paragraph, every code block, every table, all preserved as structured Markdown. It includes:

- Full content as structured Markdown
- All extracted data tables
- A cryptographic verification signature (HMAC-SHA256)

Use `.doc` when you need the **complete picture** — deep research, full analysis, or when you want the LLM to have every detail.

### `.mdoc` — Mini Document (Per-Page Index Card)

An `.mdoc` is a **token-optimized mini-document** — a concise "index card" for every indexing page on a website. Think of it as the **one-page summary** that RFO generates for *each individual page* so the protocol can natively understand your content without fetching the full page every time.

Every `.mdoc` contains:
- A tight summary (first 3 paragraphs, < 500 chars)
- Estimated token count (< 1,500 tokens)
- Q&A pairs extracted from the content (up to 20)

**Why this matters**: Instead of an AI agent downloading and parsing an entire 10,000-word page, it can grab the `.mdoc` "index card" first — instantly understanding what the page is about, its key points, and whether it's worth diving deeper. This saves tokens, reduces latency, and makes content discovery efficient at web scale.

### Coordinates — Where RFO Points You

Coordinates in RFO are **semantic location markers** that tell the engine *where* in the knowledge space your request fits. Think of it like GPS coordinates, but for information instead of physical location.

When you make a handshake request, you can provide coordinates like:
```json
{
  "topic": "machine-learning",
  "language": "Python",
  "region": "Asia"
}
```

RFO then uses these coordinates to:
- **Match your local interest** — The engine finds the nearest content "location" in its knowledge graph, like how GPS finds the nearest address to your coordinates
- **Narrow down results** — Instead of returning everything about a topic, RFO points you to the most relevant content region
- **Enable location-aware AI** — If you're in Pakistan searching for documentation, RFO can bias toward content relevant to your region, language, and context

The coordinates system uses mathematical similarity (cosine distance, region clustering) to find the **nearest possible match** between what you're looking for and what exists on the web.

### Site ID — Your Domain's Digital Fingerprint

A Site ID is a **cryptographic identity** for a domain, generated using HMAC-SHA256. It combines:
- The domain name
- A secret key (only the server knows)
- A time window (rotates hourly)

This creates a unique, unforgeable identifier that proves "this content came from example.com at this time" — without revealing the secret key.

### Quality Score — Automated Content Rating

Every compiled page gets a **quality score from 0 to 100**, based on:
- Content length and structure (headings, paragraphs)
- Code blocks and technical content
- Data tables
- Link quality and structure
- AEO (Answer Engine Optimization) readiness

A score of 80+ means the content is well-structured for AI consumption. Below 50 means the page needs improvement.

### AEO — Answer Engine Optimization

AEO is the practice of structuring content so AI agents can extract direct answers. RFO's AEO system:
- Extracts Q&A pairs from headings and content
- Generates FAQ structured data (JSON-LD)
- Scores content for "featured snippet" readiness
- Optimizes for voice search queries

### .opt Domain — AI-Optimized TLD

The `.opt` domain is a purpose-built top-level domain for AI-optimized content. Websites that use `.opt` get:
- Automatic SEO/GEO/AEO metadata generation
- JSON-LD structured data
- FAQ schema generation
- Native integration with RFO's content pipeline

### Binary Protocol — Native Rust Transfer

RFO's binary protocol transfers `.doc` and `.mdoc` payloads as raw bytes with:
- 11-byte header (magic, version, type, length)
- CRC32 checksums for integrity
- ~30-40% smaller than JSON
- Streaming support for large payloads

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

### 4. Register a .opt domain

```bash
# Register and compile a .opt domain
curl -X POST http://localhost:3000/rfo/handshake \
  -H "Content-Type: application/json" \
  -H "X-API-Key: your-api-key" \
  -d "{
    \"domain_url\": \"https://mysite.opt\",
    \"coordinates\": { \"topic\": \"documentation\" },
    \"requested_payload\": \"Mdoc\",
    \"nonce\": \"$NONCE\",
    \"timestamp\": $TIMESTAMP
  }"
```

## Architecture

```
┌──────────────────────────────────────────────────────────────────────────┐
│                         RFO CORE ENGINE                                   │
│                                                                          │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────────────────────┐│
│  │  Parser   │→│ Compiler │→│  Cache   │→│     HTTP Server           ││
│  │ HTML/MD   │  │ .doc     │  │ DashMap  │  │  Axum + Middleware       ││
│  │ →Parsed   │  │ .mdoc    │  │ TTL-based│  │  Rate Limit + Auth      ││
│  └──────────┘  │ Quality  │  └──────────┘  │  CORS + Audit            ││
│                │ Score    │                 └──────────────────────────┘│
│  ┌──────────┐  └──────────┘  ┌──────────┐  ┌──────────────────────────┐│
│  │  Crypto   │               │ Telemetry│  │    WebSocket             ││
│  │ HMAC-SHA256│              │ Metrics  │  │  WsManager pub/sub       ││
│  │ SHA-512   │               │ Reports  │  │  Domain subscriptions    ││
│  │ HKDF      │               └──────────┘  └──────────────────────────┘│
│  └──────────┘                                                         │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────────────────────┐│
│  │  Domain   │  │ Pipeline │  │  Binary  │  │     Admin API            ││
│  │ .opt TLD  │  │ .doc     │  │ Protocol │  │  RBAC + Management       ││
│  │ SEO/GEO   │  │ .mdoc    │  │ CRC32    │  │  Users + Keys + Audit    ││
│  │ AEO       │  │ Generator│  │ Streaming│  │  Cache + Stats           ││
│  └──────────┘  └──────────┘  └──────────┘  └──────────────────────────┘│
│                                                                          │
│  ┌──────────────────────────────────────────────────────────────────────┐│
│  │                    PostgreSQL Database                               ││
│  │  sites │ handshake_logs │ audit_logs │ admin_users │ api_key_records ││
│  └──────────────────────────────────────────────────────────────────────┘│
└──────────────────────────────────────────────────────────────────────────┘
```

## .opt Domain — AI-Optimized TLD

The `.opt` domain is a purpose-built TLD for AI-optimized content delivery:

```
┌──────────────────────────────────────────────────────────────────────┐
│                      .opt DOMAIN ARCHITECTURE                        │
├──────────────────────────────────────────────────────────────────────┤
│                                                                      │
│   Website Owner                  RFO Protocol                       │
│      │                               │                               │
│      │  1. Create .doc pages        │                               │
│      │  2. Create .mdoc pages       │                               │
│      │  3. Register .opt domain     │                               │
│      │  ──── POST /rfo/handshake ──▶│                               │
│      │                               │  Compile + Verify + Score    │
│      │  ◀─── Site ID + Quality ─────│                               │
│      │                               │                               │
│   AI Agent                          │                               │
│      │  ──── GET /rfo/doc ─────────▶│  Full knowledge payload      │
│      │  ◀─── .doc payload ─────────│  (deep content + verification)│
│      │                               │                               │
│      │  ──── GET /rfo/mdoc ────────▶│  Token-optimized payload     │
│      │  ◀─── .mdoc payload ────────│  (< 1,500 tokens + Q&A)      │
│      │                               │                               │
│      │  ──── GET /rfo/ws ──────────▶│  Real-time updates           │
│      │  ◀─── Live notifications ────│  (domain changes)            │
│                                                                      │
├──────────────────────────────────────────────────────────────────────┤
│  SEO: Structured data, metadata, canonical URLs                      │
│  GEO: LLM-friendly content, direct answers, structured data         │
│  AEO: Q&A pairs, FAQ schema, featured snippets                      │
└──────────────────────────────────────────────────────────────────────┘
```

## API Endpoints

### Public (No Auth)
| Endpoint | Method | Description |
|----------|--------|-------------|
| `/rfo/health` | GET | Health check & protocol version |
| `/rfo/capabilities` | GET | Server capabilities & features |
| `/rfo/negotiate` | POST | Capability negotiation |
| `/rfo/ws` | GET | WebSocket (real-time updates) |

### Protected (API Key Required)
| Endpoint | Method | Description |
|----------|--------|-------------|
| `/rfo/handshake` | POST | Duplex handshake (compile domain) |
| `/rfo/batch-handshake` | POST | Batch handshake (up to 20 domains) |
| `/rfo/doc/{domain}` | GET | Full `.doc` payload |
| `/rfo/mdoc/{domain}` | GET | Mini `.mdoc` payload |
| `/rfo/stream/{domain}` | GET | Stream `.doc` (binary) |
| `/rfo/stream-mdoc/{domain}` | GET | Stream `.mdoc` (binary) |
| `/rfo/sites` | GET | List registered sites |
| `/rfo/telemetry` | GET | Telemetry dashboard |

### Admin (Admin Auth Required)
| Endpoint | Method | Description |
|----------|--------|-------------|
| `/rfo/admin/login` | POST | Admin login (returns JWT token) |
| `/rfo/admin/users` | POST | Create admin user |
| `/rfo/admin/users/{id}/password` | PUT | Change password |
| `/rfo/admin/stats` | GET | System statistics |
| `/rfo/admin/sites` | GET | List sites (paginated, searchable) |
| `/rfo/admin/sites/{domain}` | DELETE | Delete site |
| `/rfo/admin/audit` | GET | Audit logs (paginated, filterable) |
| `/rfo/admin/keys` | GET/POST | List/create API keys |
| `/rfo/admin/keys/{name}` | DELETE | Revoke API key |
| `/rfo/admin/cache/purge` | POST | Purge cache |
| `/rfo/admin/health` | GET | Detailed health check |

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
│   ├── crypto/
│   │   ├── mod.rs           # HMAC-SHA256/SHA512, HKDF, content integrity
│   │   └── site_id.rs       # HMAC-SHA256 site ID generation
│   ├── domain.rs            # .opt domain support, SEO/GEO/AEO metadata
│   ├── pipeline.rs          # Document pipeline (.doc/.mdoc generator)
│   ├── binary.rs            # Binary protocol (native Rust transfer)
│   ├── cache/mod.rs         # DashMap concurrent cache with TTL
│   ├── auth.rs              # API key store, HMAC signing, middleware
│   ├── audit.rs             # Audit logger, DDoS protection, CORS
│   ├── admin.rs             # Admin API (RBAC, user/key management)
│   ├── telemetry.rs         # Metrics, quality trends, reports
│   ├── client.rs            # Rust client SDK
│   ├── cli.rs               # CLI (compile, watch, serve, inspect, audit)
│   └── server/
│       ├── mod.rs
│       ├── handlers.rs      # HTTP route handlers
│       ├── middleware.rs     # Rate limiting, IP extraction
│       └── websocket.rs     # WebSocket pub/sub manager
├── benches/
│   └── rfo_benchmarks.rs    # Criterion benchmarks (parser, compiler, cache, crypto, e2e)
├── tests/
│   ├── integration.rs       # Full HTTP stack tests (16)
│   ├── security.rs          # Security tests (45)
│   ├── concurrency.rs       # Race condition tests (11)
│   └── protocol.rs          # Protocol compliance tests (20)
├── migrations/
│   ├── 001_initial.sql      # sites, handshake_logs
│   ├── 002_audit_logs.sql   # audit_logs
│   └── 003_admin_architecture.sql  # admin_users, admin_sessions, api_key_records
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
# Run all 200 tests
cargo test

# Run specific suite
cargo test --test security      # 45 security tests
cargo test --test integration   # 16 integration tests
cargo test --test concurrency   # 11 concurrency tests
cargo test --test protocol      # 20 protocol compliance tests

# Run benchmarks
cargo bench
```

## Security

- **HMAC-SHA256/SHA512** — Request signing and verification
- **HKDF Key Derivation** — Secure key expansion from secrets
- **Content Integrity** — SHA-256 hashing for payload verification
- **API Key Authentication** — `X-API-Key` header on all protected endpoints
- **HMAC Request Signing** — SHA-256 body integrity verification
- **Nonce Replay Protection** — 5-minute freshness window
- **DDoS Protection** — Per-IP (100/min) + global (1000) connection limits
- **Prompt Injection Defense** — 16-pattern sanitizer (EN + ZH)
- **Audit Logging** — All security events logged to PostgreSQL
- **CORS** — Configurable allowed origins
- **Read-Only Container** — Docker runs as non-root, read-only filesystem
- **Admin RBAC** — Role-based access control for management API

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

## Roadmap

- [x] Phase 1: Core Engine Foundation
- [x] Phase 2: PostgreSQL Live Wiring
- [x] Phase 3: Client SDK
- [x] Phase 4: Advanced Cache & Telemetry
- [x] Phase 5: Content Pipeline CLI
- [x] Phase 6: Security Hardening
- [x] Phase 7: Content pipeline extensions
- [x] Phase 8: Protocol Extensions (streaming, WebSocket, capability negotiation)
- [x] Phase 9: Production Deployment (Docker, CI/CD, OpenAPI)
- [x] Phase 10: Testing & Hardening (200 tests, bugs fixed)
- [x] Phase 11: Documentation & Architecture
- [x] Phase 12: Benchmarks, Admin API & Architecture
- [x] Phase 13: .opt Domain Support (SEO/GEO/AEO)
- [x] Phase 14: Document Pipeline (.doc/.mdoc generator)
- [x] Phase 15: Binary Protocol (native Rust transfer)
- [x] Phase 16: Production Cryptography (HMAC, HKDF, content integrity)
- [x] Phase 17: Final verification & launch readiness

## License

MIT License. See [LICENSE](LICENSE).

---

<div align="center">

**Built for the AI agents that will shape the future of the web.**

</div>
