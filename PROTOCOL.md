# RFO Protocol Specification

**Version 1.0.0**

This document defines the wire format, protocols, and data schemas for the RFO (Research For Optimization) protocol.

---

## Table of Contents

1. [Overview](#overview)
2. [Protocol Versioning](#protocol-versioning)
3. [Content Types](#content-types)
4. [Handshake Protocol](#handshake-protocol)
5. [Payload Schemas](#payload-schemas)
6. [.opt Domain Protocol](#opt-domain-protocol)
7. [Binary Wire Format](#binary-wire-format)
8. [Capability Negotiation](#capability-negotiation)
9. [Streaming Protocol](#streaming-protocol)
10. [WebSocket Protocol](#websocket-protocol)
11. [Authentication](#authentication)
12. [Admin API](#admin-api)
13. [Error Handling](#error-handling)

---

## Overview

RFO is an application-layer protocol that enables AI agents to efficiently discover, verify, and consume structured web content. The protocol operates over HTTP/1.1 and WebSocket.

### Protocol Flow

```
 Agent                                    Engine
  │                                         │
  │  ┌──────────────────────────────────┐   │
  │  │  1. CAPABILITY NEGOTIATION       │   │
  │  │  POST /rfo/negotiate             │   │
  │  │  { encodings, features, version }│   │
  │  │  ◀─── { negotiated, features }   │   │
  │  └──────────────────────────────────┘   │
  │                                         │
  │  ┌──────────────────────────────────┐   │
  │  │  2. HANDSHAKE                    │   │
  │  │  POST /rfo/handshake             │   │
  │  │  { domain_url, nonce, timestamp }│   │
  │  │  ◀─── { header, payload }        │   │
  │  └──────────────────────────────────┘   │
  │                                         │
  │  ┌──────────────────────────────────┐   │
  │  │  3. PAYLOAD RETRIEVAL            │   │
  │  │  GET /rfo/doc/{domain}           │   │
  │  │  ◀─── FullDocPayload             │   │
  │  │  - or -                          │   │
  │  │  GET /rfo/mdoc/{domain}          │   │
  │  │  ◀─── MiniDocPayload             │   │
  │  └──────────────────────────────────┘   │
  │                                         │
  │  ┌──────────────────────────────────┐   │
  │  │  4. BINARY STREAMING (optional)  │   │
  │  │  GET /rfo/stream/{domain}        │   │
  │  │  ◀─── BinaryHeader + payload     │   │
  │  └──────────────────────────────────┘   │
  │                                         │
  │  ┌──────────────────────────────────┐   │
  │  │  5. REAL-TIME UPDATES (optional) │   │
  │  │  GET /rfo/ws                     │   │
  │  │  { type: "subscribe", ... }      │   │
  │  │  ◀─── { type: "update", ... }    │   │
  │  └──────────────────────────────────┘   │
  │                                         │
```

---

## Protocol Versioning

The protocol uses semantic versioning: `MAJOR.MINOR.PATCH`

- **MAJOR**: Breaking changes (incompatible wire format)
- **MINOR**: New features (backward-compatible)
- **PATCH**: Bug fixes

```json
{
  "protocol_version": "1.0.0",
  "min_supported": "1.0.0"
}
```

**Compatibility rule**: A client is compatible if `MAJOR` matches and version ≥ minimum supported version.

```
Client 1.0.0 ↔ Server 1.0.0  ✓ Compatible
Client 1.0.0 ↔ Server 1.1.0  ✓ Compatible (minor bump)
Client 1.0.0 ↔ Server 2.0.0  ✗ Incompatible (major bump)
Client 2.0.0 ↔ Server 1.0.0  ✗ Incompatible (major mismatch)
```

---

## Content Types

| Type | MIME | Description |
|------|------|-------------|
| JSON | `application/json` | Default encoding |
| MessagePack | `application/msgpack` | Binary encoding (smaller payloads) |
| Binary | `application/octet-stream` | Native Rust binary protocol |

Content type is negotiated via `POST /rfo/negotiate`.

---

## Handshake Protocol

The handshake is the core of RFO. The agent provides a domain URL, and the engine compiles, verifies, and returns structured content.

### Request

```
POST /rfo/handshake
Content-Type: application/json
X-API-Key: <your-api-key>
```

```json
{
  "domain_url": "https://example.com",
  "coordinates": {
    "topic": "machine-learning",
    "language": "Python"
  },
  "requested_payload": "Mdoc",
  "nonce": "550e8400-e29b-41d4-a716-446655440000",
  "timestamp": 1700000000
}
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `domain_url` | string (URI) | Yes | Target URL to compile |
| `coordinates` | object | Yes | Client-provided semantic coordinates |
| `requested_payload` | enum | Yes | `"Doc"` or `"Mdoc"` |
| `nonce` | string (UUID) | Yes | Replay protection nonce (≥16 chars) |
| `timestamp` | integer | Yes | Unix timestamp (within 5 minutes of server time) |

### Response

```json
{
  "header": {
    "site_id": "a1b2c3d4... (64 hex chars)",
    "coordinates": {
      "title": "Example Domain",
      "language": "HTML",
      "category": "General"
    },
    "quality_score": 85
  },
  "payload": {
    "summary": "This is a summary...",
    "token_count": 120,
    "qa_pairs": [
      {
        "question": "What is this page about?",
        "answer": "Example Domain provides..."
      }
    ]
  },
  "processing_time_ms": 142,
  "nonce": "550e8400-e29b-41d4-a716-446655440000"
}
```

### Nonce Freshness

Nonces are verified against a **5-minute window**:

```
|---5 min---|---NOW---|---5 min---|
   rejected     ✓      rejected
```

This prevents replay attacks while allowing legitimate retries.

---

## Payload Schemas

### MiniDocPayload (`.mdoc`)

Token-optimized for LLM context windows (< 1,500 tokens).

```json
{
  "summary": "Concise summary of the page content...",
  "token_count": 120,
  "qa_pairs": [
    {
      "question": "What is RFO?",
      "answer": "A protocol for AI agents to consume web content."
    }
  ]
}
```

| Field | Type | Description |
|-------|------|-------------|
| `summary` | string | First 3 paragraphs, concatenated |
| `token_count` | integer | Estimated token count (~4 chars/token) |
| `qa_pairs` | array | Question-answer pairs (max 20) |

### FullDocPayload (`.doc`)

Deep knowledge layout with full markdown and verification.

```json
{
  "raw_markdown": "# Title\n\nFull content as structured markdown...",
  "data_tables": [
    "| Feature | Status |\n| --- | --- |\n| Cache | Ready |"
  ],
  "verification_signature": "hmac-sha256-signature (64 hex chars)"
}
```

| Field | Type | Description |
|-------|------|-------------|
| `raw_markdown` | string | Full content as structured markdown |
| `data_tables` | array | Extracted data tables (markdown format) |
| `verification_signature` | string | HMAC-SHA256 for content verification |

### RfoHeader

```json
{
  "site_id": "a1b2c3d4... (64 hex chars)",
  "coordinates": {
    "topic": "Installation",
    "language": "Rust",
    "category": "Web"
  },
  "quality_score": 85
}
```

| Field | Type | Description |
|-------|------|-------------|
| `site_id` | string | HMAC-SHA256 of `{domain}\|{hour_window}` |
| `coordinates` | object | Semantic coordinates (auto-extracted + client-provided) |
| `quality_score` | integer (0-100) | Content quality score |

---

## .opt Domain Protocol

The `.opt` domain is a purpose-built TLD for AI-optimized content delivery. RFO treats `.opt` domains as first-class citizens with native parsing, metadata generation, and structured data support.

### Domain Parsing

```json
{
  "subdomain": "api",
  "domain": "mysite",
  "tld": "opt",
  "full": "api.mysite.opt",
  "is_opt": true
}
```

### SEO/GEO/AEO Metadata

When a `.opt` domain is compiled, RFO automatically generates rich metadata:

```json
{
  "seo": {
    "title": "My AI-Optimized Site",
    "description": "Concise description for search engines...",
    "canonical_url": "https://mysite.opt",
    "structured_data": "{\"@type\":\"WebSite\",\"url\":\"https://mysite.opt\"}",
    "open_graph": {
      "og:title": "My AI-Optimized Site",
      "og:description": "Concise description...",
      "og:type": "website",
      "og:url": "https://mysite.opt"
    }
  },
  "geo": {
    "llm_friendly_content": true,
    "direct_answers": true,
    "structured_data_format": "JSON-LD",
    "content_freshness": "2026-01-01T00:00:00Z"
  },
  "aeo": {
    "faq_schema": true,
    "qa_pairs": [
      {
        "question": "What is this site about?",
        "answer": "This site provides AI-optimized documentation..."
      }
    ],
    "featured_snippet_ready": true,
    "voice_search_optimized": true
  }
}
```

### JSON-LD Schema Generation

For `.opt` domains, RFO automatically generates JSON-LD schemas:

```json
{
  "@context": "https://schema.org",
  "@type": "WebSite",
  "url": "https://mysite.opt",
  "name": "My AI-Optimized Site",
  "description": "Concise description...",
  "potentialAction": {
    "@type": "SearchAction",
    "target": "https://mysite.opt/search?q={search_term_string}",
    "query-input": "required name=search_term_string"
  }
}
```

### FAQ Schema

RFO generates FAQ structured data for AEO-optimized content:

```json
{
  "@context": "https://schema.org",
  "@type": "FAQPage",
  "mainEntity": [
    {
      "@type": "Question",
      "name": "What is RFO?",
      "acceptedAnswer": {
        "@type": "Answer",
        "text": "RFO is a protocol for AI agents to consume web content."
      }
    }
  ]
}
```

### Handshake with .opt Domain

```bash
curl -X POST http://localhost:3000/rfo/handshake \
  -H "Content-Type: application/json" \
  -H "X-API-Key: your-api-key" \
  -d '{
    "domain_url": "https://mysite.opt",
    "coordinates": { "topic": "documentation" },
    "requested_payload": "Mdoc",
    "nonce": "550e8400-e29b-41d4-a716-446655440000",
    "timestamp": 1700000000
  }'
```

The engine will:
1. Parse the `.opt` domain and extract metadata
2. Generate SEO/GEO/AEO structured data
3. Create JSON-LD and FAQ schemas
4. Compile `.doc` and `.mdoc` with rich metadata
5. Return the site with quality score reflecting AEO readiness

---

## Binary Wire Format

RFO supports a native binary protocol for efficient `.doc`/`.mdoc` transfer via Rust. This is used for streaming endpoints and high-throughput scenarios.

### Binary Header (11 bytes)

```
Offset  Size  Field       Description
0       4     magic       Magic bytes: 0x52464F00 (RFO\0)
4       2     version     Protocol version (0x0001 = v1)
6       1     payload_type  0x01 = .mdoc, 0x02 = .doc, 0x03 = batch
7       4     length      Payload length in bytes (little-endian)
```

### Payload Type Markers

| Marker | Type | Description |
|--------|------|-------------|
| `0x01` | `.mdoc` | Mini document payload |
| `0x02` | `.doc` | Full document payload |
| `0x03` | batch | Batch of payloads |

### CRC32 Checksum

Each payload includes a CRC32 checksum for integrity verification:

```
[BinaryHeader][Payload Data][CRC32 (4 bytes)]
```

The CRC32 is computed over the payload data bytes and appended after the payload.

### Serialization

```rust
// Serialize a .mdoc payload
let header = BinaryHeader::new(PayloadType::Mdoc, payload_bytes.len() as u32);
let mut buffer = Vec::new();
header.write_to(&mut buffer)?;
buffer.extend_from_slice(&payload_bytes);
let checksum = crc32fast::hash(&payload_bytes);
buffer.extend_from_slice(&checksum.to_le_bytes());

// Deserialize
let (header, payload, checksum) = BinaryProtocol::deserialize(&buffer)?;
assert!(BinaryProtocol::verify_checksum(payload, checksum));
```

### Batch Binary Format

For batch transfers, multiple payloads are serialized sequentially:

```
[Header1][Payload1][CRC1][Header2][Payload2][CRC2]...
```

Each header indicates the payload type and length, allowing the receiver to parse each payload independently.

---

## Capability Negotiation

Before handshake, agents can negotiate encoding and features.

### Request

```
POST /rfo/negotiate
Content-Type: application/json
```

```json
{
  "supported_encodings": ["application/json", "application/msgpack"],
  "supported_features": ["handshake", "websocket", "streaming"],
  "protocol_version": "1.0.0"
}
```

### Response

```json
{
  "negotiated_encoding": "application/msgpack",
  "supported_features": ["handshake", "websocket"],
  "protocol_version": "1.0.0",
  "server_capabilities": [
    "handshake",
    "batch-handshake",
    "websocket",
    "streaming",
    "opt-domain",
    "binary-protocol"
  ]
}
```

### Server Capabilities

| Feature | Description |
|---------|-------------|
| `handshake` | Duplex handshake protocol |
| `batch-handshake` | Batch handshake (up to 20 domains) |
| `websocket` | Real-time pub/sub updates |
| `streaming` | Binary chunked payload transfer |
| `opt-domain` | Native .opt domain support |
| `binary-protocol` | Binary wire format with CRC32 |
| `document-pipeline` | Automatic .doc/.mdoc generation |

---

## Streaming Protocol

For large payloads, RFO supports binary streaming with checksums.

### Stream Request

```
GET /rfo/stream/{domain}
X-API-Key: <your-api-key>
Accept: application/msgpack
```

### Stream Response

Headers:
```
X-RFO-Protocol-Version: 1.0.0
X-RFO-Site-ID: a1b2c3d4...
Content-Type: application/json
Content-Length: 1024
```

Body: Raw payload bytes.

### Binary Stream Format

For native binary streaming:

```
[BinaryHeader (11 bytes)][Payload Data][CRC32 (4 bytes)]
```

### StreamChunk Format

For chunked transfer:

```json
{
  "chunk_index": 0,
  "total_chunks": 10,
  "data": "base64-encoded-chunk-data",
  "checksum": "sha256-checksum"
}
```

---

## WebSocket Protocol

Real-time domain update notifications via WebSocket.

### Connection

```
GET /rfo/ws
Upgrade: websocket
Connection: Upgrade
```

### Message Format

All messages are JSON with a `type` field:

```json
{ "type": "<message-type>", "payload": { ... } }
```

### Client → Server Messages

#### Subscribe

```json
{
  "type": "subscribe",
  "payload": {
    "domains": ["example.com", "another.com"]
  }
}
```

#### Unsubscribe

```json
{
  "type": "unsubscribe",
  "payload": {
    "domains": ["example.com"]
  }
}
```

#### Ping

```json
{ "type": "ping" }
```

### Server → Client Messages

#### Subscribed

```json
{
  "type": "subscribed",
  "domains": ["example.com"]
}
```

#### Update

```json
{
  "type": "update",
  "payload": {
    "domain": "example.com",
    "quality_score": 92,
    "timestamp": "2024-01-01T00:00:00Z"
  }
}
```

#### Pong

```json
{ "type": "pong" }
```

#### Error

```json
{
  "type": "error",
  "payload": {
    "code": 400,
    "message": "Invalid message format"
  }
}
```

---

## Authentication

### API Key

Protected endpoints require the `X-API-Key` header:

```
GET /rfo/sites
X-API-Key: your-api-key-here
```

### Public Endpoints (No Auth)

| Endpoint | Method |
|----------|--------|
| `/rfo/health` | GET |
| `/rfo/capabilities` | GET |
| `/rfo/negotiate` | POST |
| `/rfo/ws` | GET |

### HMAC Request Signing (Optional)

For request integrity verification:

```
POST /rfo/handshake
X-Signature: <hex-encoded-hmac-sha256-of-body>
```

```rust
let signature = hmac_sha256(body, secret_key);
// signature = 64 hex characters
```

### Cryptographic Operations

RFO uses production-grade cryptography:

| Operation | Algorithm | Use Case |
|-----------|-----------|----------|
| Site ID | HMAC-SHA256 | Domain + hour window identification |
| Content Hash | SHA-256 | Payload integrity verification |
| Request Signing | HMAC-SHA256 | Request body tampering detection |
| Key Derivation | HKDF-SHA256 | Secure key expansion |
| Content Integrity | HMAC-SHA256 | Domain-bound content verification |

---

## Admin API

The Admin API provides management capabilities for the RFO engine. All admin endpoints are nested under `/rfo/admin/*` and use separate authentication.

### Authentication

Admin endpoints use JWT tokens. Obtain a token via `/rfo/admin/login`:

```
POST /rfo/admin/login
Content-Type: application/json

{
  "username": "admin",
  "password": "secure-password"
}
```

Response:
```json
{
  "token": "eyJhbGciOiJIUzI1NiIs...",
  "role": "admin",
  "expires_at": "2026-01-01T12:00:00Z"
}
```

### RBAC Roles

| Role | Permissions |
|------|-------------|
| `admin` | Full access (users, keys, cache, audit, stats) |
| `operator` | Read-only access (stats, audit, keys) |
| `viewer` | Read-only (stats, audit) |

### Endpoints

| Endpoint | Method | Role | Description |
|----------|--------|------|-------------|
| `/rfo/admin/login` | POST | public | Admin login (returns JWT) |
| `/rfo/admin/users` | POST | admin | Create admin user |
| `/rfo/admin/users/{id}/password` | PUT | admin | Change password |
| `/rfo/admin/stats` | GET | operator | System statistics |
| `/rfo/admin/sites` | GET | operator | List sites (paginated, searchable) |
| `/rfo/admin/sites/{domain}` | DELETE | admin | Delete site |
| `/rfo/admin/audit` | GET | operator | Audit logs (paginated, filterable) |
| `/rfo/admin/keys` | GET/POST | admin | List/create API keys |
| `/rfo/admin/keys/{name}` | DELETE | admin | Revoke API key |
| `/rfo/admin/cache/purge` | POST | admin | Purge cache |
| `/rfo/admin/health` | GET | operator | Detailed health check |

---

## Error Handling

### HTTP Status Codes

| Code | Meaning |
|------|---------|
| `200` | Success |
| `400` | Bad Request (validation error) |
| `401` | Unauthorized (missing/invalid API key) |
| `403` | Forbidden (insufficient permissions) |
| `404` | Not Found (domain not compiled) |
| `422` | Unprocessable Entity (malformed JSON) |
| `429` | Too Many Requests (rate limited) |
| `500` | Internal Server Error |
| `502` | Bad Gateway (failed to fetch target URL) |
| `504` | Gateway Timeout |

### Error Response Format

```json
{
  "error": "Human-readable error message"
}
```

### Rate Limiting

- **Per-IP**: 100 requests/minute (configurable)
- **Global**: 1000 requests/minute (configurable)
- Returns `429 Too Many Requests` when exceeded

---

## Wire Format Examples

### Full Handshake Exchange

```
→ POST /rfo/handshake
  Content-Type: application/json
  X-API-Key: agent_alpha key_abc123

  {
    "domain_url": "https://docs.rs/axum/latest/axum/",
    "coordinates": { "language": "Rust" },
    "requested_payload": "Mdoc",
    "nonce": "550e8400-e29b-41d4-a716-446655440000",
    "timestamp": 1700000000
  }

← 200 OK
  X-RFO-Protocol-Version: 1.0.0
  X-RFO-Site-ID: a1b2c3d4e5f6...

  {
    "header": {
      "site_id": "a1b2c3d4...",
      "coordinates": {
        "title": "Axum",
        "language": "Rust",
        "category": "Web"
      },
      "quality_score": 92
    },
    "payload": {
      "summary": "Axum is a Rust web framework...",
      "token_count": 340,
      "qa_pairs": [
        {
          "question": "What is Axum?",
          "answer": "A modular, ergonomic Rust web framework..."
        }
      ]
    },
    "processing_time_ms": 89,
    "nonce": "550e8400-e29b-41d4-a716-446655440000"
  }
```

### Binary Stream Exchange

```
→ GET /rfo/stream/mysite.opt
  X-API-Key: agent_alpha key_abc123

← 200 OK
  X-RFO-Protocol-Version: 1.0.0
  X-RFO-Site-ID: a1b2c3d4e5f6...
  Content-Type: application/octet-stream
  Content-Length: 1024

  [BinaryHeader (11 bytes)][Payload][CRC32 (4 bytes)]
```
