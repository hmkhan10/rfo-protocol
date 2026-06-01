use criterion::{black_box, criterion_group, criterion_main, Criterion};
use rfo_core::compiler::{compile_mdoc, compile_doc, calculate_quality_score};
use rfo_core::parser::parse_html;
use rfo_core::crypto::site_id::generate_site_id;
use rfo_core::cache::RfoCache;
use rfo_core::rfo_protocol::{CacheEntry, FullDocPayload, MiniDocPayload, RfoHeader};
use chrono::Utc;
use std::collections::HashMap;

// ── Sample HTML Content ───────────────────────────────────────────────────────

const SIMPLE_HTML: &str = r#"
<!DOCTYPE html>
<html>
<head><title>Simple Page</title></head>
<body>
<h1>Hello World</h1>
<p>This is a simple test page for benchmarking the RFO parser.</p>
<p>It contains basic HTML structure with minimal content.</p>
</body>
</html>
"#;

const MEDIUM_HTML: &str = r#"
<!DOCTYPE html>
<html>
<head><title>Medium Page</title></head>
<body>
<h1>Comprehensive Guide to Web Development</h1>
<h2>Introduction</h2>
<p>Web development is the work involved in developing a website for the Internet or an intranet. It can range from developing a simple single static page to complex web applications.</p>
<h2>Frontend Technologies</h2>
<p>The main technologies used for frontend development are HTML, CSS, and JavaScript. Modern frameworks like React, Vue, and Angular have revolutionized how we build user interfaces.</p>
<h2>Backend Technologies</h2>
<p>Server-side programming languages include Python, Rust, Go, Node.js, and many others. Each has its own strengths and use cases.</p>
<h2>Database Systems</h2>
<p>Relational databases like PostgreSQL and MySQL store data in structured tables. NoSQL databases like MongoDB and Redis offer flexible data models.</p>
<h2>DevOps and Deployment</h2>
<p>Modern deployment practices include containerization with Docker, orchestration with Kubernetes, and CI/CD pipelines for automated testing and deployment.</p>
<table>
<tr><th>Technology</th><th>Type</th><th>Use Case</th></tr>
<tr><td>HTML</td><td>Markup</td><td>Structure</td></tr>
<tr><td>CSS</td><td>Style</td><td>Design</td></tr>
<tr><td>JavaScript</td><td>Language</td><td>Interactivity</td></tr>
<tr><td>Rust</td><td>Language</td><td>Performance</td></tr>
</table>
<pre><code>
fn main() {
    println!("Hello, world!");
}
</code></pre>
</body>
</html>
"#;

const LARGE_HTML: &str = r#"
<!DOCTYPE html>
<html>
<head><title>Large Documentation Page</title></head>
<body>
<h1>RFO Protocol Documentation</h1>
<h2>Overview</h2>
<p>The RFO Protocol is a next-generation application-layer network protocol designed for AI web-scale agents. It provides structured, verified, and token-optimized content delivery.</p>
<h2>Architecture</h2>
<p>The protocol operates over HTTP/1.1 and WebSocket, supporting both JSON and MessagePack encodings for maximum efficiency.</p>
<h3>Components</h3>
<ul>
<li>Parser: HTML/Markdown content extraction</li>
<li>Compiler: Content to .doc/.mdoc conversion</li>
<li>Cache: High-performance in-memory caching</li>
<li>Crypto: HMAC-SHA256 verification</li>
<li>Telemetry: Metrics and quality tracking</li>
</ul>
<h2>Handshake Protocol</h2>
<p>The duplex handshake is the core of RFO. The agent provides a domain URL, and the engine compiles, verifies, and returns structured content.</p>
<h3>Request Format</h3>
<pre><code>
{
    "domain_url": "https://example.com",
    "coordinates": {},
    "requested_payload": "Mdoc",
    "nonce": "uuid-v4",
    "timestamp": 1700000000
}
</code></pre>
<h3>Response Format</h3>
<pre><code>
{
    "header": {
        "site_id": "64-char-hex",
        "coordinates": {},
        "quality_score": 85
    },
    "payload": {
        "summary": "...",
        "token_count": 120,
        "qa_pairs": [...]
    }
}
</code></pre>
<h2>Security</h2>
<p>RFO implements multiple security layers including API key authentication, HMAC request signing, nonce replay protection, and rate limiting.</p>
<h3>Authentication</h3>
<p>All protected endpoints require the X-API-Key header. The API key is validated against a secure store.</p>
<h3>Rate Limiting</h3>
<p>Per-IP rate limiting (100 requests/minute) and global rate limiting (1000 requests/minute) prevent abuse.</p>
<h3>DDoS Protection</h3>
<p>The DDoS protection system monitors connection rates and blocks malicious traffic automatically.</p>
<h2>Payload Types</h2>
<h3>MiniDoc (.mdoc)</h3>
<p>Token-optimized for LLM context windows, containing a summary, token count, and Q&A pairs. Maximum 1,500 tokens.</p>
<h3>FullDoc (.doc)</h3>
<p>Complete knowledge layout with raw markdown, data tables, and verification signature for content authenticity.</p>
<table>
<tr><th>Feature</th><th>.mdoc</th><th>.doc</th></tr>
<tr><td>Token Count</td><td>&lt; 1,500</td><td>Unlimited</td></tr>
<tr><td>Q&A Pairs</td><td>Yes</td><td>No</td></tr>
<tr><td>Raw Markdown</td><td>No</td><td>Yes</td></tr>
<tr><td>Data Tables</td><td>No</td><td>Yes</td></tr>
<tr><td>Verification</td><td>No</td><td>HMAC-SHA256</td></tr>
</table>
<h2>WebSocket</h2>
<p>Real-time domain update notifications via WebSocket pub/sub. Agents can subscribe to specific domains and receive instant updates.</p>
<h3>Message Types</h3>
<ul>
<li>subscribe: Subscribe to domain updates</li>
<li>unsubscribe: Unsubscribe from domain updates</li>
<li>update: Domain content updated</li>
<li>ping/pong: Connection keepalive</li>
<li>error: Error notification</li>
</ul>
<h2>Performance</h2>
<p>The protocol is optimized for high throughput with features like connection pooling, caching, and binary encoding support.</p>
<h3>Benchmarks</h3>
<p>Handshake processing: ~100ms average, Cache hit: ~1ms, Parser: ~50ms for medium HTML.</p>
</body>
</html>
"#;

// ── Helper ────────────────────────────────────────────────────────────────────

fn make_cache_entry(mdoc: MiniDocPayload, doc: FullDocPayload, quality: u8) -> CacheEntry {
    CacheEntry {
        header: RfoHeader::new(
            "bench-site-id".to_string(),
            HashMap::new(),
            quality,
        ),
        doc,
        mdoc,
        cached_at: Utc::now(),
    }
}

fn rand_key() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64
}

// ── Benchmark Functions ───────────────────────────────────────────────────────

fn bench_parser(c: &mut Criterion) {
    let mut group = c.benchmark_group("parser");

    group.bench_function("simple_html", |b| {
        b.iter(|| parse_html(black_box(SIMPLE_HTML)))
    });

    group.bench_function("medium_html", |b| {
        b.iter(|| parse_html(black_box(MEDIUM_HTML)))
    });

    group.bench_function("large_html", |b| {
        b.iter(|| parse_html(black_box(LARGE_HTML)))
    });

    group.finish();
}

fn bench_compiler(c: &mut Criterion) {
    let mut group = c.benchmark_group("compiler");

    let simple_parsed = parse_html(SIMPLE_HTML);
    let medium_parsed = parse_html(MEDIUM_HTML);
    let large_parsed = parse_html(LARGE_HTML);

    group.bench_function("compile_mdoc_simple", |b| {
        b.iter(|| compile_mdoc(black_box(&simple_parsed)))
    });

    group.bench_function("compile_mdoc_medium", |b| {
        b.iter(|| compile_mdoc(black_box(&medium_parsed)))
    });

    group.bench_function("compile_mdoc_large", |b| {
        b.iter(|| compile_mdoc(black_box(&large_parsed)))
    });

    group.bench_function("compile_doc_simple", |b| {
        b.iter(|| compile_doc(black_box(&simple_parsed), "https://simple.example.com"))
    });

    group.bench_function("compile_doc_medium", |b| {
        b.iter(|| compile_doc(black_box(&medium_parsed), "https://medium.example.com"))
    });

    group.bench_function("compile_doc_large", |b| {
        b.iter(|| compile_doc(black_box(&large_parsed), "https://large.example.com"))
    });

    group.finish();
}

fn bench_quality_score(c: &mut Criterion) {
    let mut group = c.benchmark_group("quality_score");

    let simple_parsed = parse_html(SIMPLE_HTML);
    let medium_parsed = parse_html(MEDIUM_HTML);
    let large_parsed = parse_html(LARGE_HTML);

    let simple_mdoc = compile_mdoc(&simple_parsed);
    let simple_doc = compile_doc(&simple_parsed, "https://simple.example.com");
    let medium_mdoc = compile_mdoc(&medium_parsed);
    let medium_doc = compile_doc(&medium_parsed, "https://medium.example.com");
    let large_mdoc = compile_mdoc(&large_parsed);
    let large_doc = compile_doc(&large_parsed, "https://large.example.com");

    group.bench_function("simple", |b| {
        b.iter(|| calculate_quality_score(black_box(&simple_mdoc), black_box(&simple_doc)))
    });

    group.bench_function("medium", |b| {
        b.iter(|| calculate_quality_score(black_box(&medium_mdoc), black_box(&medium_doc)))
    });

    group.bench_function("large", |b| {
        b.iter(|| calculate_quality_score(black_box(&large_mdoc), black_box(&large_doc)))
    });

    group.finish();
}

fn bench_crypto(c: &mut Criterion) {
    let mut group = c.benchmark_group("crypto");

    // generate_site_id reads RFO_SECRET_KEY from env, set it for benchmarks
    std::env::set_var("RFO_SECRET_KEY", "benchmark-secret-key-for-testing-only");

    group.bench_function("generate_site_id", |b| {
        b.iter(|| generate_site_id(black_box("example.com")))
    });

    group.finish();
}

fn bench_cache(c: &mut Criterion) {
    let mut group = c.benchmark_group("cache");

    let cache = RfoCache::new();
    let parsed = parse_html(MEDIUM_HTML);
    let mdoc = compile_mdoc(&parsed);
    let doc = compile_doc(&parsed, "https://bench.example.com");

    // Pre-populate cache for read benchmarks
    for i in 0..1000 {
        let entry = make_cache_entry(mdoc.clone(), doc.clone(), 85);
        cache.insert(format!("example{}.com", i), entry);
    }

    group.bench_function("insert", |b| {
        b.iter_batched(
            || {
                let entry = make_cache_entry(mdoc.clone(), doc.clone(), 85);
                (format!("bench{}.com", rand_key()), entry)
            },
            |(key, entry)| cache.insert(key, entry),
            criterion::BatchSize::SmallInput,
        )
    });

    group.bench_function("get_hit", |b| {
        b.iter(|| cache.get(black_box("example100.com")))
    });

    group.bench_function("get_miss", |b| {
        b.iter(|| cache.get(black_box("nonexistent.com")))
    });

    group.bench_function("remove", |b| {
        // Re-insert before each remove
        b.iter_batched(
            || {
                let key = format!("temp{}.com", rand_key());
                let entry = make_cache_entry(mdoc.clone(), doc.clone(), 85);
                cache.insert(key.clone(), entry);
                key
            },
            |key| cache.remove(&key),
            criterion::BatchSize::SmallInput,
        )
    });

    group.finish();
}

fn bench_end_to_end(c: &mut Criterion) {
    let mut group = c.benchmark_group("end_to_end");

    std::env::set_var("RFO_SECRET_KEY", "benchmark-secret-key-for-testing-only");

    group.bench_function("parse_compile_mdoc", |b| {
        b.iter(|| {
            let parsed = parse_html(black_box(MEDIUM_HTML));
            compile_mdoc(&parsed)
        })
    });

    group.bench_function("parse_compile_doc", |b| {
        b.iter(|| {
            let parsed = parse_html(black_box(MEDIUM_HTML));
            compile_doc(&parsed, "https://bench.example.com")
        })
    });

    group.bench_function("full_pipeline", |b| {
        let cache = RfoCache::new();
        b.iter(|| {
            let parsed = parse_html(black_box(MEDIUM_HTML));
            let mdoc = compile_mdoc(&parsed);
            let doc = compile_doc(&parsed, "https://bench.example.com");
            let quality = calculate_quality_score(&mdoc, &doc);
            let entry = make_cache_entry(mdoc, doc, quality);
            cache.insert("bench.example.com".to_string(), entry);
            cache.get("bench.example.com")
        })
    });

    group.finish();
}

// ── Criterion Group ───────────────────────────────────────────────────────────

criterion_group!(
    benches,
    bench_parser,
    bench_compiler,
    bench_quality_score,
    bench_crypto,
    bench_cache,
    bench_end_to_end,
);

criterion_main!(benches);
