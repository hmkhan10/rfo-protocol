use std::sync::Arc;

use chrono::Utc;
use dashmap::DashMap;

use crate::rfo_protocol::CacheEntry;

const DEFAULT_TTL_SECS: i64 = 300; // 5 minutes

#[derive(Clone)]
pub struct RfoCache {
    inner: Arc<DashMap<String, CacheEntry>>,
    ttl_seconds: i64,
}

impl RfoCache {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(DashMap::new()),
            ttl_seconds: DEFAULT_TTL_SECS,
        }
    }

    pub fn with_ttl(ttl_seconds: i64) -> Self {
        Self {
            inner: Arc::new(DashMap::new()),
            ttl_seconds,
        }
    }

    /// Insert a compiled cache entry keyed by domain URL.
    pub fn insert(&self, key: String, entry: CacheEntry) {
        self.inner.insert(key, entry);
    }

    /// Retrieve a cached entry if it exists and hasn't expired.
    pub fn get(&self, key: &str) -> Option<CacheEntry> {
        self.inner.get(key).and_then(|entry| {
            let age = Utc::now()
                .signed_duration_since(entry.cached_at)
                .num_seconds();
            if age < self.ttl_seconds {
                Some(entry.clone())
            } else {
                // Expired — drop it lazily
                drop(entry);
                self.inner.remove(key);
                None
            }
        })
    }

    /// Retrieve a cached entry by partial domain match.
    /// Finds any entry whose key contains the given domain string.
    pub fn get_by_domain(&self, domain: &str) -> Option<CacheEntry> {
        for entry in self.inner.iter() {
            if entry.key().contains(domain) {
                let age = Utc::now()
                    .signed_duration_since(entry.value().cached_at)
                    .num_seconds();
                if age < self.ttl_seconds {
                    return Some(entry.value().clone());
                }
            }
        }
        None
    }

    /// Check if a key exists (ignoring TTL).
    pub fn contains(&self, key: &str) -> bool {
        self.inner.contains_key(key)
    }

    /// Remove a specific entry.
    pub fn remove(&self, key: &str) -> Option<CacheEntry> {
        self.inner.remove(key).map(|(_, v)| v)
    }

    /// Evict all expired entries. Call periodically from a background task.
    pub fn cleanup_expired(&self) {
        let now = Utc::now();
        self.inner.retain(|_, entry| {
            let age = now.signed_duration_since(entry.cached_at).num_seconds();
            age < self.ttl_seconds
        });
    }

    /// Current number of entries (including potentially expired ones).
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Remove all entries from the cache.
    pub fn clear(&self) {
        self.inner.clear();
    }
}

impl Default for RfoCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rfo_protocol::{FullDocPayload, MiniDocPayload, RfoHeader};
    use std::collections::HashMap;

    fn make_entry() -> CacheEntry {
        CacheEntry {
            header: RfoHeader::new("test-id".into(), HashMap::new(), 85),
            doc: FullDocPayload {
                raw_markdown: "# Test".into(),
                data_tables: vec![],
                verification_signature: "sig".into(),
            },
            mdoc: MiniDocPayload {
                summary: "Test summary".into(),
                token_count: 50,
                qa_pairs: vec![],
            },
            cached_at: Utc::now(),
        }
    }

    #[test]
    fn test_insert_and_get() {
        let cache = RfoCache::new();
        cache.insert("example.com".into(), make_entry());
        assert!(cache.get("example.com").is_some());
    }

    #[test]
    fn test_expired_entry_returns_none() {
        let cache = RfoCache::with_ttl(0); // TTL = 0 => always expired
        cache.insert("example.com".into(), make_entry());
        // Small sleep to ensure time passes
        std::thread::sleep(std::time::Duration::from_millis(10));
        assert!(cache.get("example.com").is_none());
    }

    #[test]
    fn test_cleanup_expired() {
        let cache = RfoCache::with_ttl(0);
        cache.insert("a.com".into(), make_entry());
        cache.insert("b.com".into(), make_entry());
        std::thread::sleep(std::time::Duration::from_millis(10));
        cache.cleanup_expired();
        assert_eq!(cache.len(), 0);
    }
}
