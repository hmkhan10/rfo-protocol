// ─────────────────────────────────────────────────────────────────────────────
// RFO Protocol — Concurrency Tests
// ─────────────────────────────────────────────────────────────────────────────
// Testing thread safety, race conditions, and concurrent access patterns.
// ─────────────────────────────────────────────────────────────────────────────

use std::sync::Arc;
use std::time::Duration;

use rfo_core::audit::DdosProtection;
use rfo_core::cache::RfoCache;
use rfo_core::rfo_protocol::{CacheEntry, FullDocPayload, MiniDocPayload, RfoHeader};
use rfo_core::server::websocket::WsManager;
use rfo_core::telemetry::TelemetryTracker;
use std::collections::HashMap;

fn make_entry(domain: &str) -> CacheEntry {
    CacheEntry {
        header: RfoHeader::new(format!("id-{}", domain), HashMap::new(), 80),
        doc: FullDocPayload {
            raw_markdown: format!("# {}", domain),
            data_tables: vec![],
            verification_signature: "sig".to_string(),
        },
        mdoc: MiniDocPayload {
            summary: format!("Summary of {}", domain),
            token_count: 50,
            qa_pairs: vec![],
        },
        cached_at: chrono::Utc::now(),
    }
}

// ── Cache Concurrent Access ─────────────────────────────────────────────────

#[tokio::test]
async fn test_cache_concurrent_inserts() {
    let cache = Arc::new(RfoCache::new());
    let mut handles = vec![];

    for i in 0..100 {
        let cache = cache.clone();
        handles.push(tokio::spawn(async move {
            cache.insert(format!("domain-{}.com", i), make_entry(&format!("domain-{}.com", i)));
        }));
    }

    for handle in handles {
        handle.await.unwrap();
    }

    assert_eq!(cache.len(), 100);
}

#[tokio::test]
async fn test_cache_concurrent_reads_and_writes() {
    let cache = Arc::new(RfoCache::new());
    let mut handles = vec![];

    // Writers
    for i in 0..50 {
        let cache = cache.clone();
        handles.push(tokio::spawn(async move {
            cache.insert(format!("writer-{}.com", i), make_entry("test"));
        }));
    }

    // Readers
    for i in 0..50 {
        let cache = cache.clone();
        handles.push(tokio::spawn(async move {
            let _ = cache.get(&format!("writer-{}.com", i));
            let _ = cache.get_by_domain("writer");
        }));
    }

    for handle in handles {
        handle.await.unwrap();
    }
}

#[tokio::test]
async fn test_cache_concurrent_cleanup() {
    let cache = Arc::new(RfoCache::with_ttl(0)); // TTL = 0, everything expires
    let mut handles = vec![];

    // Insert entries
    for i in 0..50 {
        cache.insert(format!("expired-{}.com", i), make_entry("test"));
    }

    // Concurrent cleanup + reads
    for i in 0..10 {
        let cache = cache.clone();
        handles.push(tokio::spawn(async move {
            cache.cleanup_expired();
            let _ = cache.get(&format!("expired-{}.com", i));
        }));
    }

    for handle in handles {
        handle.await.unwrap();
    }

    // After cleanup, all should be gone
    assert_eq!(cache.len(), 0);
}

// ── Telemetry Concurrent Access ─────────────────────────────────────────────

#[tokio::test]
async fn test_telemetry_concurrent_recordings() {
    let tracker = Arc::new(TelemetryTracker::new());
    let mut handles = vec![];

    // Concurrent cache hits
    for _ in 0..100 {
        let tracker = tracker.clone();
        handles.push(tokio::spawn(async move {
            tracker.record_cache_hit();
        }));
    }

    // Concurrent cache misses
    for _ in 0..100 {
        let tracker = tracker.clone();
        handles.push(tokio::spawn(async move {
            tracker.record_cache_miss();
        }));
    }

    // Concurrent quality scores
    for i in 0..50 {
        let tracker = tracker.clone();
        handles.push(tokio::spawn(async move {
            tracker.record_quality_score(
                &format!("site-{}", i),
                &format!("domain-{}.com", i),
                (i % 100) as u32,
            );
        }));
    }

    // Concurrent handshakes
    for i in 0..50 {
        let tracker = tracker.clone();
        handles.push(tokio::spawn(async move {
            tracker.record_handshake(&format!("site-{}", i));
        }));
    }

    for handle in handles {
        handle.await.unwrap();
    }

    let report = tracker.report(0);
    assert_eq!(report.cache.hits, 100);
    assert_eq!(report.cache.misses, 100);
    assert!(report.top_sites.len() <= 10); // Report truncates to top 10
}

#[tokio::test]
async fn test_telemetry_ring_buffer_overflow() {
    let tracker = Arc::new(TelemetryTracker::new());
    let mut handles = vec![];

    // Record 200 requests (ring buffer is 100)
    for i in 0..200 {
        let tracker = tracker.clone();
        handles.push(tokio::spawn(async move {
            tracker.record_request(rfo_core::telemetry::RequestTelemetry {
                domain: format!("domain-{}.com", i),
                cache_hit: i % 2 == 0,
                processing_time_ms: i as u64,
                payload_type: "Mdoc".to_string(),
                timestamp: chrono::Utc::now(),
            });
        }));
    }

    for handle in handles {
        handle.await.unwrap();
    }

    let report = tracker.report(0);
    // Should only keep last 100 (or 20 in report)
    assert!(report.recent_requests.len() <= 20);
}

// ── WebSocket Manager Concurrent Access ──────────────────────────────────────

#[tokio::test]
async fn test_ws_concurrent_subscribes() {
    let manager = Arc::new(WsManager::new());
    let mut handles = vec![];

    for i in 0..100 {
        let manager = manager.clone();
        handles.push(tokio::spawn(async move {
            manager.subscribe_domain(&format!("domain-{}.com", i));
        }));
    }

    for handle in handles {
        handle.await.unwrap();
    }

    // Each domain should have 1 subscriber
    for i in 0..100 {
        assert_eq!(
            manager.subscriber_count(&format!("domain-{}.com", i)),
            1
        );
    }
}

#[tokio::test]
async fn test_ws_concurrent_publish_and_subscribe() {
    let manager = Arc::new(WsManager::new());
    let mut handles = vec![];

    // Subscribers
    for i in 0..10 {
        let manager = manager.clone();
        handles.push(tokio::spawn(async move {
            let _rx = manager.subscribe_domain(&format!("hot-domain-{}.com", i));
            // Keep the receiver alive briefly
            tokio::time::sleep(Duration::from_millis(50)).await;
        }));
    }

    // Publishers
    for i in 0..100 {
        let manager = manager.clone();
        handles.push(tokio::spawn(async move {
            manager.publish_update(&format!("hot-domain-{}.com", i % 10), 85);
        }));
    }

    for handle in handles {
        handle.await.unwrap();
    }
}

// ── DDoS Protection Concurrent Access ───────────────────────────────────────

#[tokio::test]
async fn test_ddos_concurrent_connections() {
    let ddos = Arc::new(DdosProtection::new(50, 200));
    let mut handles = vec![];

    // 200 concurrent connections from different IPs
    for i in 0..200 {
        let ddos = ddos.clone();
        handles.push(tokio::spawn(async move {
            let allowed = ddos.check_connection(&format!("10.0.0.{}", i % 256));
            allowed
        }));
    }

    let mut allowed = 0;
    let mut _blocked = 0;
    for handle in handles {
        if handle.await.unwrap() {
            allowed += 1;
        } else {
            _blocked += 1;
        }
    }

    // Global limit is 200, so most should be allowed
    assert!(allowed > 0);
    // Some may be blocked due to per-IP limits
}

#[tokio::test]
async fn test_ddos_release_under_contention() {
    let ddos = Arc::new(DdosProtection::new(5, 100));
    let mut handles = vec![];

    // Fill up one IP
    for _ in 0..5 {
        ddos.check_connection("10.0.0.1");
    }

    // Concurrent release + check
    for _ in 0..10 {
        let ddos = ddos.clone();
        handles.push(tokio::spawn(async move {
            ddos.release_connection("10.0.0.1");
            ddos.check_connection("10.0.0.1");
        }));
    }

    for handle in handles {
        handle.await.unwrap();
    }

    // After releases, should have room
    assert!(ddos.active_connections() <= 100);
}

// ── Cache Clone Safety ──────────────────────────────────────────────────────

#[tokio::test]
async fn test_cache_clone_independence() {
    let cache1 = RfoCache::new();
    let cache2 = cache1.clone();

    cache1.insert("key1".to_string(), make_entry("key1"));
    cache2.insert("key2".to_string(), make_entry("key2"));

    // Both should see both entries (shared state)
    assert!(cache1.get("key1").is_some());
    assert!(cache1.get("key2").is_some());
    assert!(cache2.get("key1").is_some());
    assert!(cache2.get("key2").is_some());
}

// ── Telemetry Clone Safety ──────────────────────────────────────────────────

#[tokio::test]
async fn test_telemetry_clone_shares_state() {
    let t1 = TelemetryTracker::new();
    let t2 = t1.clone();

    t1.record_cache_hit();
    t2.record_cache_hit();
    t1.record_cache_miss();

    let report = t1.report(0);
    assert_eq!(report.cache.hits, 2);
    assert_eq!(report.cache.misses, 1);
}
