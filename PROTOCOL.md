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
6. [Capability Negotiation](#capability-negotiation)
7. [Streaming Protocol](#streaming-protocol)
8. [WebSocket Protocol](#websocket-protocol)
9. [Authentication](#authentication)
10. [Error Handling](#error-handling)

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
  │  │  4. REAL-TIME UPDATES (optional) │   │
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
| `site_id` | string | HMAC-SHA256 of `{domain}|{hour_window}` |
| `coordinates` | object | Semantic coordinates (auto-extracted + client-provided) |
| `quality_score` | integer (0-100) | Content quality score |

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
    "streaming"
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

---

## Error Handling

### HTTP Status Codes

| Code | Meaning |
|------|---------|
| `200` | Success |
| `400` | Bad Request (validation error) |
| `401` | Unauthorized (missing/invalid API key) |
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
