# RFO SDK Guide

**Rust Client SDK for the RFO Protocol**

---

## Table of Contents

1. [Installation](#installation)
2. [Quick Start](#quick-start)
3. [Client Builder](#client-builder)
4. [Handshake](#handshake)
5. [Batch Handshake](#batch-handshake)
6. [Payload Retrieval](#payload-retrieval)
7. [Capability Negotiation](#capability-negotiation)
8. [WebSocket](#websocket)
9. [Error Handling](#error-handling)
10. [Examples](#examples)

---

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
rfo-core = "1.0.0"
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

For MessagePack support:

```toml
[dependencies]
rfo-core = "1.0.0"
rmp-serde = "1"
```

---

## Quick Start

```rust
use rfo_core::client::RfoClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = RfoClient::new("http://localhost:3000", "your-api-key")?;

    let response = client.handshake("https://example.com").await?;

    println!("Site ID: {}", response.header.site_id);
    println!("Quality: {}", response.header.quality_score);
    println!("Summary: {}", response.payload.summary);

    Ok(())
}
```

---

## Client Builder

Use the builder pattern for advanced configuration:

```rust
use rfo_core::client::RfoClient;
use std::time::Duration;

let client = RfoClientBuilder::new()
    .base_url("https://rfo.example.com")
    .api_key("your-api-key")
    .timeout(Duration::from_secs(10))
    .max_retries(3)
    .retry_delay(Duration::from_millis(500))
    .build()?;
```

### Options

| Option | Default | Description |
|--------|---------|-------------|
| `base_url` | `http://localhost:3000` | RFO Engine URL |
| `api_key` | none | API key for authentication |
| `timeout` | 30s | Request timeout |
| `max_retries` | 3 | Retry count on transient failures |
| `retry_delay` | 500ms | Delay between retries |

---

## Handshake

Perform a duplex handshake to compile a domain:

```rust
use rfo_core::client::RfoClient;
use rfo_core::rfo_protocol::Coordinates;

let client = RfoClient::new("http://localhost:3000", "api-key")?;

let response = client.handshake("https://example.com").await?;

// Access the header
println!("Site ID: {}", response.header.site_id);
println!("Quality: {}", response.header.quality_score);
println!("Coordinates: {:?}", response.header.coordinates);

// Access the payload (MiniDoc)
println!("Summary: {}", response.payload.summary);
println!("Tokens: {}", response.payload.token_count);

for qa in &response.payload.qa_pairs {
    println!("Q: {}", qa.question);
    println!("A: {}", qa.answer);
}
```

### With Custom Coordinates

```rust
let response = client.handshake_with_coordinates(
    "https://docs.rs/axum/latest/axum/",
    Coordinates {
        topic: Some("web-framework".to_string()),
        language: Some("Rust".to_string()),
    },
).await?;
```

---

## Batch Handshake

Compile multiple domains in a single request:

```rust
let urls = vec![
    "https://example.com",
    "https://docs.rs/axum",
    "https://crates.io",
];

let responses = client.batch_handshake(urls).await?;

for response in responses {
    match response {
        Ok(resp) => println!("{}: quality={}", resp.header.site_id, resp.header.quality_score),
        Err(e) => eprintln!("Error: {}", e),
    }
}
```

---

## Payload Retrieval

Retrieve previously compiled payloads without recompiling:

### Full `.doc` Payload

```rust
let doc = client.get_doc("example.com").await?;

println!("Markdown: {}", doc.raw_markdown);
println!("Tables: {:?}", doc.data_tables);
println!("Signature: {}", doc.verification_signature);
```

### Mini `.mdoc` Payload

```rust
let mdoc = client.get_mdoc("example.com").await?;

println!("Summary: {}", mdoc.summary);
println!("Tokens: {}", mdoc.token_count);
println!("QA Pairs: {}", mdoc.qa_pairs.len());
```

### List Registered Sites

```rust
let sites = client.list_sites().await?;

for site in sites {
    println!("{}: quality={}", site.domain, site.quality_score);
}
```

---

## Capability Negotiation

Query server capabilities before handshake:

```rust
use rfo_core::protocol::{CapabilityRequest, PayloadEncoding};

let request = CapabilityRequest {
    supported_encodings: vec![
        PayloadEncoding::Json,
        PayloadEncoding::MsgPack,
    ],
    supported_features: vec![
        "handshake".to_string(),
        "websocket".to_string(),
    ],
    protocol_version: "1.0.0".to_string(),
};

let response = client.negotiate(request).await?;

println!("Negotiated encoding: {:?}", response.negotiated_encoding);
println!("Server capabilities: {:?}", response.server_capabilities);
```

---

## WebSocket

Connect to real-time domain updates:

```rust
use rfo_core::protocol::WsMessage;

let mut ws = client.connect_websocket().await?;

// Subscribe to domains
ws.send(WsMessage::Subscribe {
    payload: SubscribePayload {
        domains: vec!["example.com".to_string()],
    },
}).await?;

// Receive updates
while let Some(msg) = ws.recv().await? {
    match msg {
        WsMessage::Update { payload } => {
            println!("Update: {} quality={}", payload.domain, payload.quality_score);
        }
        WsMessage::Pong => println!("Pong received"),
        WsMessage::Error { payload } => eprintln!("Error: {}", payload.message),
        _ => {}
    }
}
```

---

## Error Handling

The SDK returns `Result<T, RfoError>` for all operations:

```rust
use rfo_core::client::RfoError;

match client.handshake("https://example.com").await {
    Ok(response) => {
        // Process response
    }
    Err(RfoError::Unauthorized) => {
        eprintln!("Invalid API key");
    }
    Err(RfoError::RateLimited) => {
        eprintln!("Rate limited, retry later");
    }
    Err(RfoError::NotFound(domain)) => {
        eprintln!("Domain not found: {}", domain);
    }
    Err(RfoError::Timeout) => {
        eprintln!("Request timed out");
    }
    Err(RfoError::Network(e)) => {
        eprintln!("Network error: {}", e);
    }
    Err(e) => {
        eprintln!("Other error: {}", e);
    }
}
```

### Error Types

| Error | HTTP Code | Description |
|-------|-----------|-------------|
| `Unauthorized` | 401 | Invalid or missing API key |
| `RateLimited` | 429 | Too many requests |
| `NotFound` | 404 | Domain not compiled |
| `Timeout` | — | Request exceeded timeout |
| `Network` | — | Connection error |
| `Serialization` | — | JSON/MessagePack parse error |

---

## Examples

### Example 1: Monitor Domain Quality

```rust
use rfo_core::client::RfoClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = RfoClient::new("http://localhost:3000", "api-key")?;

    let domains = vec![
        "https://example.com",
        "https://docs.rs",
        "https://crates.io",
    ];

    for url in domains {
        let response = client.handshake(url).await?;
        println!(
            "{}: quality={}/100, tokens={}",
            url,
            response.header.quality_score,
            response.payload.token_count
        );
    }

    Ok(())
}
```

### Example 2: Stream Large Payload

```rust
use futures::StreamExt;

let mut stream = client.stream_doc("example.com").await?;

while let Some(chunk) = stream.next().await {
    let chunk = chunk?;
    println!("Chunk {}/{}: {} bytes",
        chunk.chunk_index + 1,
        chunk.total_chunks,
        chunk.data.len()
    );
}
```

### Example 3: Batch Processing with Retry

```rust
use tokio::time::{sleep, Duration};

async fn process_with_retry(
    client: &RfoClient,
    url: &str,
    max_retries: u32,
) -> Result<HandshakeResponse, RfoError> {
    for attempt in 0..max_retries {
        match client.handshake(url).await {
            Ok(response) => return Ok(response),
            Err(RfoError::RateLimited) if attempt < max_retries - 1 => {
                let delay = Duration::from_secs(2u64.pow(attempt as u32));
                sleep(delay).await;
            }
            Err(e) => return Err(e),
        }
    }
    unreachable!()
}
```

---

## Advanced Usage

### Custom HTTP Client

The SDK uses `reqwest` under the hood. For custom TLS or proxy settings:

```rust
let http_client = reqwest::Client::builder()
    .timeout(Duration::from_secs(10))
    .proxy(reqwest::Proxy::http("http://proxy:8080")?)
    .build()?;

let client = RfoClientBuilder::new()
    .base_url("http://localhost:3000")
    .api_key("api-key")
    .http_client(http_client)
    .build()?;
```

### Connection Pooling

For high-throughput scenarios, reuse the client across requests:

```rust
// Create client once
let client = Arc::new(RfoClient::new("http://localhost:3000", "api-key")?);

// Use across tasks
let client_clone = client.clone();
tokio::spawn(async move {
    client_clone.handshake("https://example.com").await
});
```

### Webhook Integration

Listen for WebSocket updates and trigger actions:

```rust
let mut ws = client.connect_websocket().await?;
ws.send(WsMessage::Subscribe {
    payload: SubscribePayload {
        domains: vec!["critical-site.com".to_string()],
    },
}).await?;

while let Some(msg) = ws.recv().await? {
    if let WsMessage::Update { payload } = msg {
        if payload.quality_score < 50 {
            alert_team(&payload.domain).await;
        }
    }
}
```
