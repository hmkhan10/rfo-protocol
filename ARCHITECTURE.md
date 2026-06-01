# RFO Architecture

**Version 1.0.0**

---

## Table of Contents

1. [System Overview](#system-overview)
2. [Component Design](#component-design)
3. [Data Flow](#data-flow)
4. [Security Model](#security-model)
5. [Database Schema](#database-schema)
6. [Design Decisions](#design-decisions)

---

## System Overview

```
┌──────────────────────────────────────────────────────────────────────────┐
│                           RFO CORE ENGINE                                 │
│                                                                          │
│  ┌─────────────┐     ┌──────────────┐     ┌──────────────┐             │
│  │    CLI       │     │  HTTP Server  │     │  WebSocket   │             │
│  │ compile      │────▶│  Axum +       │────▶│  WsManager   │             │
│  │ watch        │     │  Middleware   │     │  pub/sub     │             │
│  │ serve        │     │              │     │              │             │
│  │ inspect      │     └──────┬───────┘     └──────────────┘             │
│  │ audit        │            │                                          │
│  └─────────────┘            │                                          │
│                              ▼                                          │
│                    ┌──────────────────┐                                  │
│                    │   Request Flow    │                                  │
│                    │   (Middleware)    │                                  │
│                    └────────┬─────────┘                                  │
│                             │                                            │
│              ┌──────────────┼──────────────┐                            │
│              ▼              ▼              ▼                            │
│      ┌──────────┐  ┌──────────┐  ┌──────────┐                         │
│      │  Rate    │  │  Auth    │  │  CORS    │                          │
│      │  Limit   │  │  Key     │  │          │                          │
│      │  (per-IP)│  │  Verify  │  │          │                          │
│      └────┬─────┘  └────┬─────┘  └────┬─────┘                         │
│           │              │              │                               │
│           ▼              ▼              ▼                               │
│      ┌──────────────────────────────────────┐                          │
│      │         Handler Layer                 │                          │
│      │  /rfo/handshake  /rfo/doc  /rfo/ws   │                          │
│      └──────────────┬───────────────────────┘                          │
│                     │                                                   │
│         ┌───────────┼───────────┬──────────────┐                      │
│         ▼           ▼           ▼              ▼                      │
│    ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────────┐              │
│    │ Parser  │ │ Compiler│ │ Crypto  │ │  Telemetry  │              │
│    │ HTML/MD │ │ .doc    │ │ HMAC    │ │  Metrics    │              │
│    │ →Parsed │ │ .mdoc   │ │ Site ID │ │  Reports    │              │
│    └────┬────┘ └────┬────┘ └─────────┘ └─────────────┘              │
│         │           │                                                │
│         ▼           ▼                                                │
│    ┌──────────────────────────┐                                      │
│    │     Cache (DashMap)      │                                      │
│    │     TTL: 1 hour          │                                      │
│    │     Domain → Payload     │                                      │
│    └──────────┬───────────────┘                                      │
│               │                                                       │
│               ▼                                                       │
│    ┌──────────────────────────┐                                      │
│    │     PostgreSQL Database   │                                      │
│    │  sites │ handshake_logs  │                                      │
│    │  audit_logs │ users      │                                      │
│    └──────────────────────────┘                                      │
└──────────────────────────────────────────────────────────────────────────┘
```

---

## Component Design

### 1. Entry Point (`main.rs`)

**Responsibility**: Bootstraps either the HTTP server or CLI based on arguments.

```
rfo-core                    # No args → start HTTP server on :3000
rfo-core compile <file>     # CLI mode: compile a file
rfo-core watch <dir>        # CLI mode: watch directory for changes
rfo-core inspect <url>      # CLI mode: inspect a compiled site
rfo-core audit              # CLI mode: show audit logs
```

**Design choice**: Single binary with dual mode avoids deployment complexity. Same codebase, same tests.

### 2. HTTP Server (`server/`)

**Framework**: Axum 0.7

**Routing** (12 endpoints):

```
GET  /rfo/health          → health_check          (public)
GET  /rfo/capabilities    → capabilities          (public)
POST /rfo/negotiate       → negotiate             (public)
POST /rfo/handshake       → handshake             (protected)
POST /rfo/batch-handshake → batch_handshake       (protected)
GET  /rfo/doc/:domain     → doc                   (protected)
GET  /rfo/mdoc/:domain    → mdoc                  (protected)
GET  /rfo/stream/:domain  → stream                (protected)
GET  /rfo/stream-mdoc/:domain → stream_mdoc       (protected)
GET  /rfo/sites           → list_sites            (protected)
GET  /rfo/telemetry       → telemetry             (protected)
GET  /rfo/ws              → ws_handler            (public)
```

### 3. Middleware Stack

Request flow:

```
Request
  │
  ├─▶ Layer 1: RequestBodyLimitLayer (10MB max body size)
  │
  ├─▶ Layer 2: TimeoutLayer (30s)
  │
  ├─▶ Layer 3: CorsLayer (configurable origins)
  │
  ├─▶ Layer 4: api_key_middleware
  │    ├─ Skip: /rfo/health, /rfo/capabilities, /rfo/negotiate, /rfo/ws
  │    ├─ Check: X-API-Key header present
  │    ├─ Validate: API key exists in ApiKeyStore
  │    └─ Reject: 401 Unauthorized
  │
  ├─▶ Layer 5: ip_extraction_middleware
  │    └─ Extract real IP from X-Forwarded-For (or connection)
  │
  ├─▶ Layer 6: rate_limit_middleware
  │    ├─ Per-IP: 100 requests/minute
  │    ├─ Global: 1000 requests/minute
  │    └─ Reject: 429 Too Many Requests
  │
  └─▶ Handler
```

### 4. Parser (`parser.rs`)

**Responsibility**: Convert raw HTML/Markdown → structured `ParsedContent`.

**Features**:
- HTML entity decoding
- `<table>` → data extraction
- `<pre><code>` → code block preservation
- Link extraction (deduplicated)
- Prompt injection detection (16 patterns, EN + ZH)

**Output**:

```rust
pub struct ParsedContent {
    pub paragraphs: Vec<String>,
    pub code_blocks: Vec<String>,
    pub data_tables: Vec<Vec<Vec<String>>>,
    pub links: Vec<String>,
    pub has_tables: bool,
    pub has_code: bool,
    pub links_external: usize,
    pub links_internal: usize,
}
```

### 5. Compiler (`compiler.rs`)

**Responsibility**: Convert `ParsedContent` → `.doc` or `.mdoc` payloads.

**Compilation pipeline**:

```
ParsedContent
    │
    ├─▶ Generate summary (first 3 paragraphs, truncated to 500 chars)
    │
    ├─▶ Generate Q&A pairs (up to 20, using heading extraction)
    │
    ├─▶ Calculate quality score (0-100)
    │    ├─ Length score (0-30)
    │    ├─ Heading score (0-20)
    │    ├─ Code score (0-20)
    │    ├─ Table score (0-15)
    │    ├─ Link score (0-15)
    │
    ├─▶ .mdoc: Serialize to MiniDocPayload
    │    └─ < 1,500 tokens
    │
    └─▶ .doc: Serialize to FullDocPayload
         ├─ raw_markdown: Full content
         ├─ data_tables: Extracted tables
         └─ verification_signature: HMAC-SHA256
```

### 6. Crypto (`crypto/site_id.rs`)

**Responsibility**: Generate deterministic, time-windowed site identifiers.

**Algorithm**:

```rust
fn generate_site_id(domain: &str, secret: &str) -> String {
    let hour_window = Utc::now().hour();
    let payload = format!("{}|{}", domain, hour_window);
    hmac_sha256(payload, secret)  // Returns 64 hex chars
}
```

**Properties**:
- Deterministic: Same domain + hour → same site_id
- Rotates hourly: Prevents long-term tracking
- Secret-dependent: Unforgeable without secret key

### 7. Auth (`auth.rs`)

**Responsibility**: API key management and HMAC request signing.

```rust
pub struct ApiKeyStore {
    keys: DashMap<String, ApiKeyInfo>,
}

pub struct ApiKeyInfo {
    pub key_hash: String,      // SHA-256 of raw key
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub permissions: Vec<String>,
}
```

**Auth middleware flow**:

```
Request with X-API-Key header
    │
    ├─▶ Is public endpoint? → Yes → Skip auth
    │
    ├─▶ Extract key from header
    │
    ├─▶ Hash key (SHA-256)
    │
    ├─▶ Lookup in ApiKeyStore
    │
    ├─▶ Found? → Pass through
    │
    └─▶ Not found → 401 Unauthorized
```

### 8. Audit (`audit.rs`)

**Responsibility**: Structured security event logging and DDoS protection.

**Events logged**:
- `AuthFailure` — Invalid/missing API key
- `DdosHit` — Rate limit exceeded
- `SuspiciousHandshake` — Replay detection
- `PayloadServed` — Content delivered
- `ConfigChange` — Server configuration updated

**DDoS protection**:

```rust
pub struct DdosProtection {
    per_ip: DashMap<IpAddr, RateLimit>,
    global: AtomicUsize,
    max_per_ip: u32,          // Default: 100/min
    max_global: u32,          // Default: 1000/min
}
```

### 9. Cache (`cache/mod.rs`)

**Responsibility**: High-performance in-memory caching with TTL.

```rust
pub struct RfoCache {
    entries: DashMap<String, CacheEntry>,
    ttl: Duration,            // Default: 1 hour
}

struct CacheEntry {
    payload: CachePayload,
    created_at: Instant,
}
```

**Eviction**: Lazy — expired entries removed on access. No background threads.

### 10. Telemetry (`telemetry.rs`)

**Responsibility**: Request metrics, quality trends, and reporting.

```rust
pub struct TelemetryTracker {
    requests: AtomicU64,
    cache_hits: AtomicU64,
    cache_misses: AtomicU64,
    errors: AtomicU64,
    total_processing_ms: AtomicU64,
    domain_counts: DashMap<String, AtomicU64>,
    quality_trends: DashMap<String, QualityTrend>,
}
```

### 11. WebSocket (`server/websocket.rs`)

**Responsibility**: Real-time pub/sub for domain updates.

```rust
pub struct WsManager {
    subscribers: DashMap<String, Vec<broadcast::Sender<WsMessage>>>,
}
```

**Message flow**:

```
Client ──subscribe──▶ WsManager
                         │
                         ▼
                    Domain update occurs
                         │
                         ▼
                    WsManager broadcasts to all subscribers
                         │
                         ▼
                    Client ◀── update message
```

---

## Data Flow

### Handshake Flow (End-to-End)

```
1. Agent sends POST /rfo/handshake
   { domain_url: "https://example.com", nonce: "abc", timestamp: 1700000000 }

2. Middleware validates:
   ├─ Body size < 10MB ✓
   ├─ Not timed out (30s) ✓
   ├─ API key valid ✓
   ├─ Rate limit OK ✓
   └─ Nonce fresh (< 5 min) ✓

3. Handler calls compile_doc():
   ├─ Check cache → Miss
   ├─ HTTP GET example.com → 200 OK, HTML content
   ├─ Parser extracts:
   │    paragraphs: ["Welcome to Example...", "This domain is..."]
   │    links: ["https://iana.org/domains/example"]
   │    has_code: false, has_tables: false
   ├─ Compiler generates:
   │    summary: "Welcome to Example Domain. This domain is..."
   │    qa_pairs: [{ q: "What is Example Domain?", a: "..." }]
   │    quality_score: 45
   │    verification_signature: "hmac-sha256..."
   └─ Cache stored: "example.com" → payload

4. Response sent:
   { header: { site_id, coordinates, quality_score },
     payload: { summary, token_count, qa_pairs } }
```

### Cache Hit Flow

```
1. Agent sends GET /rfo/doc/example.com

2. Middleware validates (same as above)

3. Handler checks cache:
   └─ Cache HIT → Return cached payload (0ms processing)

4. Response sent with X-Cache: HIT header
```

### WebSocket Update Flow

```
1. Agent connects: GET /rfo/ws → WebSocket upgrade

2. Agent subscribes:
   { type: "subscribe", domains: ["example.com"] }

3. Server stores subscription in WsManager

4. Domain example.com is recompiled (via CLI or API):

5. WsManager broadcasts to all subscribers:
   { type: "update",
     payload: { domain: "example.com",
                quality_score: 88,
                timestamp: "2024-01-01T00:00:00Z" } }

6. Agent receives real-time notification
```

---

## Security Model

### Threat Model

| Threat | Mitigation |
|--------|------------|
| Replay attacks | Nonce freshness (5 min window) |
| API key theft | SHA-256 hashing, not stored in plaintext |
| Rate abuse | Per-IP + global rate limiting |
| Prompt injection | 16-pattern sanitizer (EN + ZH) |
| Content spoofing | HMAC-SHA256 verification signatures |
| DDoS | Global connection limits, request throttling |
| Unauthorized access | API key middleware on protected endpoints |

### Authentication Layers

```
Layer 1: Transport Security (TLS in production)
    │
Layer 2: API Key Authentication
    │   ├─ Header: X-API-Key
    │   ├─ Hashed (SHA-256) before storage
    │   └─ Checked via DashMap lookup
    │
Layer 3: HMAC Request Signing (optional)
    │   ├─ Header: X-Signature
    │   ├─ HMAC-SHA256(body, secret_key)
    │   └─ Prevents body tampering
    │
Layer 4: Nonce Freshness
    │   ├─ UUID v4 in request
    │   ├─ Server checks timestamp ± 5 min
    │   └─ Prevents replay attacks
    │
Layer 5: Rate Limiting
    │   ├─ Per-IP: 100 req/min
    │   ├─ Global: 1000 req/min
    │   └─ Returns 429 Too Many Requests
```

### Prompt Injection Defense

The parser detects and neutralizes 16 injection patterns:

**English patterns** (10):
- `ignore all instructions`
- `disregard previous`
- `forget everything`
- `new instructions`
- `override instructions`
- `forget your instructions`
- `ignore previous instructions`
- `ignore all previous`
- `you are now`
- `act as`

**Chinese patterns** (6):
- `忽略所有指令`
- `忽略之前的指令`
- `忘记之前`
- `你是现在`
- `假装是`
- `覆盖指令`

---

## Database Schema

### sites

```sql
CREATE TABLE sites (
    id BIGSERIAL PRIMARY KEY,
    domain TEXT UNIQUE NOT NULL,
    quality_score INTEGER DEFAULT 0,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);
```

### handshake_logs

```sql
CREATE TABLE handshake_logs (
    id BIGSERIAL PRIMARY KEY,
    site_id TEXT NOT NULL,
    domain TEXT NOT NULL,
    client_ip INET,
    processing_time_ms INTEGER,
    quality_score INTEGER,
    success BOOLEAN DEFAULT true,
    error_message TEXT,
    created_at TIMESTAMPTZ DEFAULT NOW()
);
```

### audit_logs

```sql
CREATE TABLE audit_logs (
    id BIGSERIAL PRIMARY KEY,
    event_type TEXT NOT NULL,
    client_ip INET,
    details JSONB DEFAULT '{}',
    severity TEXT DEFAULT 'info',
    created_at TIMESTAMPTZ DEFAULT NOW()
);
```

---

## Design Decisions

### 1. DashMap over Arc<RwLock<HashMap>>

**Choice**: DashMap (sharded concurrent HashMap)

**Reason**: DashMap provides fine-grained locking with automatic sharding. Read-heavy workloads (cache lookups) don't block each other, and writes only lock the relevant shard.

### 2. Runtime SQLx over Compile-Time `query!` macros

**Choice**: Runtime `sqlx::query().fetch_one()` instead of `query!`

**Reason**: Compile-time `query!` macros require a live database connection during `cargo build`. This adds complexity to CI/CD and local development. Runtime queries fail at first request instead of compile time, which is acceptable for this use case.

### 3. Single Binary with Dual Mode

**Choice**: Same `rfo-core` binary serves as server (no args) or CLI (with subcommands)

**Reason**: Reduces deployment artifacts. One binary to build, test, and ship. The binary is ~15MB in release mode.

### 4. MessagePack via rmp-serde

**Choice**: MessagePack binary encoding via rmp-serde

**Reason**: ~30-40% smaller than JSON for the same data. Important for streaming large payloads and reducing token counts for LLM consumption.

### 5. Broadcast Channels for WebSocket

**Choice**: `tokio::sync::broadcast` channels per domain

**Reason**: Broadcast channels are simple, efficient, and support multiple subscribers per domain. Each domain gets its own channel, so updates to one domain don't interfere with others.

### 6. Lazy Cache Eviction

**Choice**: Expired entries removed on access, no background threads

**Reason**: Simpler implementation, no need for periodic timers. In practice, cache access frequency is high enough that expired entries are cleaned up quickly.

### 7. Public Endpoint Bypass in Auth

**Choice**: Auth middleware skips public endpoints

**Reason**: Health checks and capability negotiation must work without authentication. This allows load balancers and monitoring systems to check server status without API keys.

### 8. Content-Type Strings in Serde

**Choice**: `PayloadEncoding` serializes as `"application/json"` not `"Json"`

**Reason**: Direct content-type strings are more self-documenting in wire format and avoid ambiguity with MIME type parsing.
