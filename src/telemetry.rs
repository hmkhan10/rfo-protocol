use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};

// ── Cache Metrics ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheMetrics {
    pub hits: u64,
    pub misses: u64,
    pub hit_ratio: f64,
    pub total_entries: usize,
    pub entries_expired: u64,
}

// ── Quality Score Trend ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityTrend {
    pub site_id: String,
    pub domain: String,
    pub current_score: u8,
    pub history: Vec<ScoreSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoreSnapshot {
    pub score: u8,
    pub recorded_at: DateTime<Utc>,
}

// ── Per-Request Telemetry ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestTelemetry {
    pub domain: String,
    pub cache_hit: bool,
    pub processing_time_ms: u64,
    pub payload_type: String,
    pub timestamp: DateTime<Utc>,
}

// ── Aggregate Telemetry Report ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryReport {
    pub cache: CacheMetrics,
    pub quality_trends: Vec<QualityTrend>,
    pub recent_requests: Vec<RequestTelemetry>,
    pub top_sites: Vec<TopSite>,
    pub uptime_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopSite {
    pub site_id: String,
    pub domain: String,
    pub quality_score: u8,
    pub total_handshakes: u64,
}

// ── TelemetryTracker (thread-safe, lock-free where possible) ───────────────

#[derive(Clone)]
pub struct TelemetryTracker {
    inner: Arc<TelemetryInner>,
}

struct TelemetryInner {
    cache_hits: AtomicU64,
    cache_misses: AtomicU64,
    entries_expired: AtomicU64,
    start_time: Instant,

    /// Quality score history per site_id (capped at 50 entries per site).
    quality_history: DashMap<String, Vec<ScoreSnapshot>>,

    /// Domain lookup for quality history display.
    site_domains: DashMap<String, String>,

    /// Handshake count per site_id.
    handshake_counts: DashMap<String, AtomicU64>,

    /// Recent requests ring buffer (last 100).
    recent_requests: DashMap<usize, RequestTelemetry>,
    recent_index: AtomicU64,
}

impl TelemetryTracker {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(TelemetryInner {
                cache_hits: AtomicU64::new(0),
                cache_misses: AtomicU64::new(0),
                entries_expired: AtomicU64::new(0),
                start_time: Instant::now(),
                quality_history: DashMap::new(),
                site_domains: DashMap::new(),
                handshake_counts: DashMap::new(),
                recent_requests: DashMap::new(),
                recent_index: AtomicU64::new(0),
            }),
        }
    }

    // ── Cache Tracking ──────────────────────────────────────────────────

    pub fn record_cache_hit(&self) {
        self.inner.cache_hits.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_cache_miss(&self) {
        self.inner.cache_misses.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_expired(&self, count: u64) {
        self.inner
            .entries_expired
            .fetch_add(count, Ordering::Relaxed);
    }

    // ── Quality Score Tracking ──────────────────────────────────────────

    pub fn record_quality_score(&self, site_id: &str, domain: &str, score: u8) {
        self.inner
            .site_domains
            .insert(site_id.to_string(), domain.to_string());

        let mut history = self
            .inner
            .quality_history
            .entry(site_id.to_string())
            .or_default();

        history.push(ScoreSnapshot {
            score,
            recorded_at: Utc::now(),
        });

        // Cap history at 50 entries per site
        let len = history.len();
        if len > 50 {
            history.drain(0..len - 50);
        }
    }

    // ── Handshake Counting ──────────────────────────────────────────────

    pub fn record_handshake(&self, site_id: &str) {
        self.inner
            .handshake_counts
            .entry(site_id.to_string())
            .or_insert_with(|| AtomicU64::new(0))
            .fetch_add(1, Ordering::Relaxed);
    }

    // ── Request Telemetry ───────────────────────────────────────────────

    pub fn record_request(&self, telemetry: RequestTelemetry) {
        let idx = (self.inner.recent_index.fetch_add(1, Ordering::Relaxed) % 100) as usize;
        self.inner.recent_requests.insert(idx, telemetry);
    }

    // ── Report Generation ───────────────────────────────────────────────

    pub fn report(&self, total_entries: usize) -> TelemetryReport {
        let hits = self.inner.cache_hits.load(Ordering::Relaxed);
        let misses = self.inner.cache_misses.load(Ordering::Relaxed);
        let total_requests = hits + misses;
        let hit_ratio = if total_requests > 0 {
            hits as f64 / total_requests as f64
        } else {
            0.0
        };

        // Quality trends
        let quality_trends: Vec<QualityTrend> = self
            .inner
            .quality_history
            .iter()
            .map(|entry| {
                let site_id = entry.key().clone();
                let history = entry.value().clone();
                let current_score = history.last().map(|s| s.score).unwrap_or(0);
                let domain = self
                    .inner
                    .site_domains
                    .get(&site_id)
                    .map(|d| d.value().clone())
                    .unwrap_or_default();

                QualityTrend {
                    site_id,
                    domain,
                    current_score,
                    history,
                }
            })
            .collect();

        // Top sites by handshake count
        let mut top_sites: Vec<TopSite> = self
            .inner
            .handshake_counts
            .iter()
            .map(|entry| {
                let site_id = entry.key().clone();
                let count = entry.value().load(Ordering::Relaxed);
                let domain = self
                    .inner
                    .site_domains
                    .get(&site_id)
                    .map(|d| d.value().clone())
                    .unwrap_or_default();
                let score = self
                    .inner
                    .quality_history
                    .get(&site_id)
                    .and_then(|h| h.last().map(|s| s.score))
                    .unwrap_or(0);

                TopSite {
                    site_id,
                    domain,
                    quality_score: score,
                    total_handshakes: count,
                }
            })
            .collect();
        top_sites.sort_by(|a, b| b.total_handshakes.cmp(&a.total_handshakes));
        top_sites.truncate(10);

        // Recent requests (collect from ring buffer)
        let mut recent: Vec<RequestTelemetry> = self
            .inner
            .recent_requests
            .iter()
            .map(|entry| entry.value().clone())
            .collect();
        recent.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        recent.truncate(20);

        TelemetryReport {
            cache: CacheMetrics {
                hits,
                misses,
                hit_ratio,
                total_entries,
                entries_expired: self.inner.entries_expired.load(Ordering::Relaxed),
            },
            quality_trends,
            recent_requests: recent,
            top_sites,
            uptime_seconds: self.inner.start_time.elapsed().as_secs(),
        }
    }
}

impl Default for TelemetryTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_hit_miss_tracking() {
        let tracker = TelemetryTracker::new();
        tracker.record_cache_hit();
        tracker.record_cache_hit();
        tracker.record_cache_miss();

        let report = tracker.report(0);
        assert_eq!(report.cache.hits, 2);
        assert_eq!(report.cache.misses, 1);
        assert!((report.cache.hit_ratio - 0.666).abs() < 0.01);
    }

    #[test]
    fn test_quality_score_history() {
        let tracker = TelemetryTracker::new();
        tracker.record_quality_score("site1", "example.com", 45);
        tracker.record_quality_score("site1", "example.com", 72);
        tracker.record_quality_score("site1", "example.com", 85);

        let report = tracker.report(0);
        assert_eq!(report.quality_trends.len(), 1);
        assert_eq!(report.quality_trends[0].current_score, 85);
        assert_eq!(report.quality_trends[0].history.len(), 3);
    }

    #[test]
    fn test_handshake_counting() {
        let tracker = TelemetryTracker::new();
        tracker.record_handshake("site1");
        tracker.record_handshake("site1");
        tracker.record_handshake("site2");

        let report = tracker.report(0);
        assert_eq!(report.top_sites.len(), 2);
        assert_eq!(report.top_sites[0].total_handshakes, 2);
    }

    #[test]
    fn test_recent_requests() {
        let tracker = TelemetryTracker::new();

        for i in 0..5 {
            tracker.record_request(RequestTelemetry {
                domain: format!("example{}.com", i),
                cache_hit: i % 2 == 0,
                processing_time_ms: i * 100,
                payload_type: "Mdoc".to_string(),
                timestamp: Utc::now(),
            });
        }

        let report = tracker.report(0);
        assert_eq!(report.recent_requests.len(), 5);
    }

    #[test]
    fn test_quality_history_cap() {
        let tracker = TelemetryTracker::new();
        for i in 0..60 {
            tracker.record_quality_score("site1", "example.com", (i % 100) as u8);
        }

        let report = tracker.report(0);
        assert_eq!(report.quality_trends[0].history.len(), 50);
    }

    #[test]
    fn test_hit_ratio_zero_requests() {
        let tracker = TelemetryTracker::new();
        let report = tracker.report(0);
        assert_eq!(report.cache.hit_ratio, 0.0);
    }
}
