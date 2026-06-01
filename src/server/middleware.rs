use std::net::SocketAddr;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use axum::middleware::Next;
use axum::response::Response;
use dashmap::DashMap;

// ── Constants ──────────────────────────────────────────────────────────────

const RATE_LIMIT_WINDOW_SECS: u64 = 60;
const RATE_LIMIT_MAX_REQUESTS: u32 = 100;

// ── Rate Limiter State ─────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct RateLimitEntry {
    count: u32,
    window_start: std::time::Instant,
}

#[derive(Clone)]
pub struct RateLimitState {
    inner: Arc<DashMap<String, RateLimitEntry>>,
}

impl RateLimitState {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(DashMap::new()),
        }
    }

    /// Returns true if the request is allowed, false if rate-limited.
    fn check_rate_limit(&self, ip: &str) -> bool {
        let now = std::time::Instant::now();

        let mut entry = self.inner.entry(ip.to_string()).or_insert(RateLimitEntry {
            count: 0,
            window_start: now,
        });

        // Reset window if expired
        if now.duration_since(entry.window_start).as_secs() >= RATE_LIMIT_WINDOW_SECS {
            entry.count = 0;
            entry.window_start = now;
        }

        entry.count += 1;
        entry.count <= RATE_LIMIT_MAX_REQUESTS
    }

    /// Periodic cleanup of expired entries. Call from a background task.
    pub fn cleanup(&self) {
        let now = std::time::Instant::now();
        self.inner.retain(|_, entry| {
            now.duration_since(entry.window_start).as_secs() < RATE_LIMIT_WINDOW_SECS * 2
        });
    }
}

impl Default for RateLimitState {
    fn default() -> Self {
        Self::new()
    }
}

// ── Middleware Functions ────────────────────────────────────────────────────

/// IP-based rate limiting middleware.
/// Extracts client IP from ConnectInfo or X-Forwarded-For header.
pub async fn rate_limit_middleware(
    axum::extract::State(state): axum::extract::State<RateLimitState>,
    request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let ip = extract_client_ip(&request);

    if !state.check_rate_limit(&ip) {
        tracing::warn!("Rate limit exceeded for IP: {}", ip);
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }

    Ok(next.run(request).await)
}

/// Extracts client IP from ConnectInfo or X-Forwarded-For / X-Real-IP headers.
fn extract_client_ip<B>(req: &Request<B>) -> String {
    // Try X-Forwarded-For first (for reverse proxies)
    if let Some(forwarded) = req.headers().get("x-forwarded-for") {
        if let Ok(val) = forwarded.to_str() {
            if let Some(first_ip) = val.split(',').next() {
                return first_ip.trim().to_string();
            }
        }
    }

    // Try X-Real-IP
    if let Some(real_ip) = req.headers().get("x-real-ip") {
        if let Ok(val) = real_ip.to_str() {
            return val.trim().to_string();
        }
    }

    // Fall back to socket address (requires ConnectInfo extension)
    req.extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ci| ci.0.ip().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limit_allows_normal_traffic() {
        let state = RateLimitState::new();
        // Should allow RATE_LIMIT_MAX_REQUESTS requests
        for _ in 0..RATE_LIMIT_MAX_REQUESTS {
            assert!(state.check_rate_limit("192.168.1.1"));
        }
        // Next one should be blocked
        assert!(!state.check_rate_limit("192.168.1.1"));
    }

    #[test]
    fn test_rate_limit_independent_per_ip() {
        let state = RateLimitState::new();
        for _ in 0..RATE_LIMIT_MAX_REQUESTS {
            assert!(state.check_rate_limit("10.0.0.1"));
        }
        // Different IP should still be allowed
        assert!(state.check_rate_limit("10.0.0.2"));
    }
}
