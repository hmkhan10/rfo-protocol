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
│      │  /rfo/admin/*                        │                          │
│      └──────────────┬───────────────────────┘                          │
│                     │                                                   │
│         ┌───────────┼───────────┬──────────────┐                      │
│         ▼           ▼           ▼              ▼                      │
│    ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────────┐              │
│    │ Parser  │ │ Compiler│ │ Crypto  │ │  Telemetry  │              │
│    │ HTML/MD │ │ .doc    │ │ HMAC    │ │  Metrics    │              │
│    │ →Parsed │ │ .mdoc   │ │ Site ID │ │  Reports    │              │
│    └────┬────┘ └────┬────┘ │ HKDF    │ └─────────────┘              │
│         │           │      │ SHA-256 │                                │
│         ▼           ▼      └─────────┘                                │
│    ┌──────────────────────────┐                                        │
│    │     Cache (DashMap)      │                                        │
│    │     TTL: 1 hour          │                                        │
│    │     Domain → Payload     │                                        │
│    └──────────┬───────────────┘                                        │
│               │                                                         │
│               ▼                                                         │
│    ┌──────────────────────────┐                                        │
│    │     PostgreSQL Database   │                                        │
│    │  sites │ handshake_logs  │                                        │
│    │  audit_logs │ admin      │                                        │
│    │  api_key_records         │                                        │
│    └──────────────────────────┘                                        │
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

**Routing** (12 core + 12 admin endpoints):

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

POST /rfo/admin/login           → admin_login          (public)
POST /rfo/admin/users           → create_admin_user    (admin)
PUT  /rfo/admin/users/:id/password → change_password   (admin)
GET  /rfo/admin/stats           → get_system_stats     (operator)
GET  /rfo/admin/sites           → list_admin_sites     (operator)
DELETE /rfo/admin/sites/:domain → delete_site          (admin)
GET  /rfo/admin/audit           → list_audit_logs      (operator)
GET  /rfo/admin/keys            → list_api_keys        (operator)
POST /rfo/admin/keys            → create_api_key       (admin)
DELETE /rfo/admin/keys/:name    → revoke_api_key       (admin)
POST /rfo/admin/cache/purge     → purge_cache          (admin)
GET  /rfo/admin/health          → admin_health         (operator)
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

### 6. Crypto (`crypto/`)

**Responsibility**: Production-grade cryptographic operations.

#### Site ID Generation (`site_id.rs`)

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

#### Core Cryptography (`mod.rs`)

```rust
pub struct RfoCrypto {
    secret: Vec<u8>,
}

impl RfoCrypto {
    // HMAC operations
    fn hmac_sha256(&self, message: &[u8]) -> String;
    fn hmac_sha512(&self, message: &[u8]) -> String;
    fn verify_hmac(&self, message: &[u8], signature: &str) -> bool;

    // Hashing
    fn sha256(&self, data: &[u8]) -> String;
    fn sha512(&self, data: &[u8]) -> String;
    fn hash_content(&self, domain: &str, content: &[u8]) -> String;

    // Key derivation
    fn derive_key(&self, salt: &[u8], info: &[u8]) -> Vec<u8>;

    // Nonce operations
    fn generate_nonce(&self) -> String;
    fn verify_nonce_format(&self, nonce: &str) -> bool;
    fn verify_nonce_freshness(&self, timestamp: i64) -> bool;

    // Content integrity
    fn content_hash(&self, domain: &str, content: &[u8]) -> String;
    fn verify_content_integrity(&self, domain: &str, content: &[u8], hash: &str) -> bool;

    // Domain binding
    fn bind_domain(&self, domain: &str, content_hash: &str) -> String;
    fn verify_domain_binding(&self, domain: &str, content_hash: &str, binding: &str) -> bool;

    // Request signing
    fn sign_request(&self, method: &str, path: &str, body: &[u8]) -> String;
    fn verify_request_signature(&self, method: &str, path: &str, body: &[u8], signature: &str) -> bool;
}
```

### 7. Domain (`domain.rs`)

**Responsibility**: `.opt` domain parsing, SEO/GEO/AEO metadata generation.

```rust
pub struct RfoDomain {
    pub subdomain: Option<String>,
    pub domain: String,
    pub tld: Tld,
}

pub enum Tld {
    Standard(String),
    Opt,
}

pub struct OptMetadata {
    pub seo: SeoMetadata,
    pub geo: GeoMetadata,
    pub aeo: AeoMetadata,
}
```

**Features**:
- Parse any domain URL into structured components
- Detect `.opt` TLD natively
- Generate SEO metadata (title, description, canonical URL, structured data, Open Graph)
- Generate GEO metadata (LLM-friendly content, direct answers, content freshness)
- Generate AEO metadata (FAQ schema, Q&A pairs, featured snippet readiness)
- Generate JSON-LD schemas for structured data
- Generate FAQ schemas for AEO optimization

### 8. Pipeline (`pipeline.rs`)

**Responsibility**: Batch document generation for websites.

```rust
pub struct DocumentPipeline {
    config: PipelineConfig,
}

pub struct PipelineConfig {
    pub max_pages_per_site: usize,
    pub quality_threshold: u8,
    pub enable_aeo: bool,
    pub enable_geo: bool,
}
```

**Features**:
- Compile individual pages (`.doc` and `.mdoc`)
- Compile entire sites with statistics
- Calculate quality scores for each page
- Generate structured data for `.opt` domains
- Batch processing with configurable limits

### 9. Binary Protocol (`binary.rs`)

**Responsibility**: Native Rust binary transfer of `.doc`/`.mdoc` payloads.

```rust
pub struct BinaryHeader {
    pub magic: [u8; 4],        // 0x52464F00 (RFO\0)
    pub version: u16,          // 0x0001
    pub payload_type: u8,      // 0x01=.mdoc, 0x02=.doc, 0x03=batch
    pub length: u32,           // Payload length
}
```

**Features**:
- 11-byte header (magic + version + type + length)
- CRC32 checksums for payload integrity
- Payload type markers for .mdoc/.doc/batch
- Streaming reader for large payloads
- Batch serialization support

### 10. Auth (`auth.rs`)

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

### 11. Admin (`admin.rs`)

**Responsibility**: Admin API with RBAC, user/key management.

```rust
pub struct AdminState {
    db: PgPool,
    jwt_secret: Vec<u8>,
}

pub struct AdminUser {
    pub id: i64,
    pub username: String,
    pub password_hash: String,
    pub role: String,
    pub created_at: DateTime<Utc>,
}

pub enum AdminRole {
    Admin,      // Full access
    Operator,   // Read-only (stats, audit, keys)
    Viewer,     // Read-only (stats, audit)
}
```

### 12. Audit (`audit.rs`)

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

### 13. Cache (`cache/mod.rs`)

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

### 14. Telemetry (`telemetry.rs`)

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

### 15. WebSocket (`server/websocket.rs`)

**Responsibility**: Real-time pub/sub for domain updates.

```rust
pub struct WsManager {
    subscribers: DashMap<String, Vec<broadcast::Sender<WsMessage>>>,
}
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

### .opt Domain Flow

```
1. Agent sends POST /rfo/handshake
   { domain_url: "https://mysite.opt", ... }

2. Engine detects .opt TLD:
   ├─ Parse: subdomain=None, domain="mysite", tld=Opt
   ├─ Generate SEO metadata (title, description, canonical URL)
   ├─ Generate GEO metadata (LLM-friendly, direct answers)
   ├─ Generate AEO metadata (FAQ schema, Q&A pairs)
   ├─ Generate JSON-LD schemas
   └─ Generate FAQ structured data

3. Compile with rich metadata:
   ├─ .doc: Full content + structured data
   └─ .mdoc: Token-optimized + AEO metadata

4. Response includes .opt-specific fields
```

### Binary Stream Flow

```
1. Agent sends GET /rfo/stream/mysite.opt

2. Engine serializes to binary:
   ├─ BinaryHeader: magic=0x52464F00, version=0x0001, type=0x02, length=N
   ├─ Payload: FullDocPayload bytes
   └─ CRC32: checksum of payload bytes

3. Stream response:
   Content-Type: application/octet-stream
   [11-byte header][payload bytes][4-byte CRC32]
```

### Cache Hit Flow

```
1. Agent sends GET /rfo/doc/example.com

2. Middleware validates (same as above)

3. Handler checks cache:
   └─ Cache HIT → Return cached payload (0ms processing)

4. Response sent with X-Cache: HIT header
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
| Body tampering | HMAC request signing |
| Key compromise | HKDF key derivation, hourly rotation |

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
    │
Layer 6: Admin RBAC
    │   ├─ JWT tokens with role-based access
    │   ├─ admin / operator / viewer roles
    │   └─ Separate auth for admin endpoints
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

### admin_users

```sql
CREATE TABLE admin_users (
    id BIGSERIAL PRIMARY KEY,
    username TEXT UNIQUE NOT NULL,
    password_hash TEXT NOT NULL,
    role TEXT NOT NULL DEFAULT 'admin',
    created_at TIMESTAMPTZ DEFAULT NOW()
);
```

### admin_sessions

```sql
CREATE TABLE admin_sessions (
    id BIGSERIAL PRIMARY KEY,
    user_id BIGINT REFERENCES admin_users(id),
    token_hash TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW()
);
```

### api_key_records

```sql
CREATE TABLE api_key_records (
    id BIGSERIAL PRIMARY KEY,
    name TEXT UNIQUE NOT NULL,
    key_hash TEXT NOT NULL,
    permissions TEXT[] DEFAULT '{}',
    created_at TIMESTAMPTZ DEFAULT NOW(),
    revoked_at TIMESTAMPTZ
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

### 9. .opt as First-Class Citizen

**Choice**: `.opt` domain has native parsing and metadata generation

**Reason**: AI-optimized content needs structured metadata for LLM discovery. By building `.opt` into the protocol, websites that register `.opt` domains get automatic SEO/GEO/AEO optimization.

### 10. Binary Protocol with CRC32

**Choice**: Binary wire format with CRC32 checksums

**Reason**: Binary transfer is ~30-40% smaller than JSON. CRC32 provides fast integrity verification without the overhead of SHA-256 for every payload.

### 11. Separate Admin State

**Choice**: Admin routes use separate `AdminState` (not `AppState`)

**Reason**: Admin endpoints need different state (JWT secret, database pool) than core endpoints. Separate state avoids type mismatches in Axum routers and keeps the admin API isolated.
