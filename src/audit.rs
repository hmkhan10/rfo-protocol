use std::sync::Arc;

use dashmap::DashMap;
use sqlx::PgPool;

// ── Audit Event Types ──────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum AuditEventType {
    AuthSuccess,
    AuthFailure,
    RateLimitHit,
    SignatureValid,
    SignatureInvalid,
    HandshakeCompleted,
    BatchHandshake,
    SiteRegistered,
    PayloadAccess,
    ServerStart,
    ServerError,
}

impl AuditEventType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AuthSuccess => "auth.success",
            Self::AuthFailure => "auth.failure",
            Self::RateLimitHit => "rate_limit.hit",
            Self::SignatureValid => "signature.valid",
            Self::SignatureInvalid => "signature.invalid",
            Self::HandshakeCompleted => "handshake.completed",
            Self::BatchHandshake => "handshake.batch",
            Self::SiteRegistered => "site.registered",
            Self::PayloadAccess => "payload.access",
            Self::ServerStart => "server.start",
            Self::ServerError => "server.error",
        }
    }
}

// ── Audit Event ────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct AuditEvent {
    pub event_type: AuditEventType,
    pub severity: String,
    pub source_ip: String,
    pub api_key_name: Option<String>,
    pub endpoint: String,
    pub method: String,
    pub status_code: Option<i32>,
    pub message: String,
    pub metadata: Option<serde_json::Value>,
}

// ── Rate Limit Tracker (DDoS mitigation) ───────────────────────────────────

#[derive(Debug, Clone)]
struct ConnectionRecord {
    count: u32,
    window_start: std::time::Instant,
}

#[derive(Clone)]
pub struct DdosProtection {
    connections_per_ip: Arc<DashMap<String, ConnectionRecord>>,
    global_connections: Arc<std::sync::atomic::AtomicU64>,
    max_per_ip: u32,
    max_global: u64,
}

impl DdosProtection {
    pub fn new(max_per_ip: u32, max_global: u64) -> Self {
        Self {
            connections_per_ip: Arc::new(DashMap::new()),
            global_connections: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            max_per_ip,
            max_global,
        }
    }

    /// Check if a connection from this IP is allowed.
    pub fn check_connection(&self, ip: &str) -> bool {
        let global = self
            .global_connections
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if global >= self.max_global {
            self.global_connections
                .fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
            return false;
        }

        let now = std::time::Instant::now();
        let mut entry = self
            .connections_per_ip
            .entry(ip.to_string())
            .or_insert(ConnectionRecord {
                count: 0,
                window_start: now,
            });

        // Reset after 1 minute
        if now.duration_since(entry.window_start).as_secs() >= 60 {
            entry.count = 0;
            entry.window_start = now;
        }

        entry.count += 1;
        entry.count <= self.max_per_ip
    }

    /// Release a connection slot.
    pub fn release_connection(&self, _ip: &str) {
        self.global_connections
            .fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// Current global connection count.
    pub fn active_connections(&self) -> u64 {
        self.global_connections
            .load(std::sync::atomic::Ordering::Relaxed)
    }
}

impl Default for DdosProtection {
    fn default() -> Self {
        Self::new(100, 1000)
    }
}

// ── Audit Logger ───────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct AuditLogger {
    db: Option<PgPool>,
    buffer: Arc<DashMap<usize, AuditEvent>>,
    buffer_index: Arc<std::sync::atomic::AtomicU64>,
    ddos: DdosProtection,
}

impl AuditLogger {
    pub fn new(db: Option<PgPool>) -> Self {
        Self {
            db,
            buffer: Arc::new(DashMap::new()),
            buffer_index: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            ddos: DdosProtection::default(),
        }
    }

    pub fn with_ddos_protection(db: Option<PgPool>, ddos: DdosProtection) -> Self {
        Self {
            db,
            buffer: Arc::new(DashMap::new()),
            buffer_index: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            ddos,
        }
    }

    /// Log an audit event. Non-blocking — writes to buffer + async DB flush.
    pub fn log(&self, event: AuditEvent) {
        // Store in ring buffer
        let idx = self
            .buffer_index
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            % 500;
        self.buffer.insert(idx as usize, event.clone());

        // Async DB write
        if let Some(pool) = &self.db {
            let pool = pool.clone();
            tokio::spawn(async move {
                let result = sqlx::query(
                    r#"INSERT INTO audit_logs (event_type, severity, source_ip, api_key_name, endpoint, method, status_code, message, metadata)
                       VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"#,
                )
                .bind(event.event_type.as_str())
                .bind(&event.severity)
                .bind(&event.source_ip)
                .bind(&event.api_key_name)
                .bind(&event.endpoint)
                .bind(&event.method)
                .bind(event.status_code)
                .bind(&event.message)
                .bind(event.metadata)
                .execute(&pool)
                .await;

                if let Err(e) = result {
                    tracing::error!("Failed to write audit log: {}", e);
                }
            });
        }
    }

    /// Get recent audit events from the ring buffer.
    pub fn recent_events(&self, count: usize) -> Vec<AuditEvent> {
        let mut events: Vec<AuditEvent> = self
            .buffer
            .iter()
            .map(|entry| entry.value().clone())
            .collect();
        events.reverse();
        events.truncate(count);
        events
    }

    /// Get the DDoS protection layer.
    pub fn ddos(&self) -> &DdosProtection {
        &self.ddos
    }
}

// ── CORS Configuration ─────────────────────────────────────────────────────

/// Build CORS layer with configured allowed origins.
pub fn build_cors() -> tower_http::cors::CorsLayer {
    let mut allowed_origins = vec![];

    // Read allowed origins from environment
    if let Ok(origins_str) = std::env::var("RFO_CORS_ORIGINS") {
        for origin in origins_str.split(',') {
            let origin = origin.trim().to_string();
            if !origin.is_empty() {
                allowed_origins.push(origin);
            }
        }
    }

    // Default to localhost dev origins
    if allowed_origins.is_empty() {
        allowed_origins = vec![
            "http://localhost:3001".to_string(),
            "http://localhost:3002".to_string(),
            "http://127.0.0.1:3001".to_string(),
        ];
    }

    let mut cors = tower_http::cors::CorsLayer::new();

    for origin in &allowed_origins {
        if let Ok(uri) = origin.parse::<axum::http::HeaderValue>() {
            cors = cors.allow_origin(uri);
        }
    }

    cors.allow_methods([
        axum::http::Method::GET,
        axum::http::Method::POST,
        axum::http::Method::OPTIONS,
    ])
    .allow_headers([
        axum::http::header::CONTENT_TYPE,
        axum::http::header::AUTHORIZATION,
        axum::http::header::HeaderName::from_static("x-api-key"),
        axum::http::header::HeaderName::from_static("x-signature"),
    ])
    .max_age(std::time::Duration::from_secs(3600))
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audit_event_type_strings() {
        assert_eq!(AuditEventType::AuthSuccess.as_str(), "auth.success");
        assert_eq!(AuditEventType::RateLimitHit.as_str(), "rate_limit.hit");
        assert_eq!(
            AuditEventType::HandshakeCompleted.as_str(),
            "handshake.completed"
        );
    }

    #[test]
    fn test_ddos_protection_allows_normal() {
        let ddos = DdosProtection::new(5, 100);
        assert!(ddos.check_connection("10.0.0.1"));
        assert!(ddos.check_connection("10.0.0.1"));
        assert_eq!(ddos.active_connections(), 2);
    }

    #[test]
    fn test_ddos_protection_blocks_excessive() {
        let ddos = DdosProtection::new(3, 100);
        assert!(ddos.check_connection("10.0.0.1"));
        assert!(ddos.check_connection("10.0.0.1"));
        assert!(ddos.check_connection("10.0.0.1"));
        assert!(!ddos.check_connection("10.0.0.1")); // 4th blocked
    }

    #[test]
    fn test_ddos_protection_global_limit() {
        let ddos = DdosProtection::new(100, 3);
        assert!(ddos.check_connection("10.0.0.1"));
        assert!(ddos.check_connection("10.0.0.2"));
        assert!(ddos.check_connection("10.0.0.3"));
        assert!(!ddos.check_connection("10.0.0.4")); // Global limit hit
    }

    #[test]
    fn test_audit_logger_buffer() {
        let logger = AuditLogger::new(None);
        for i in 0..5 {
            logger.log(AuditEvent {
                event_type: AuditEventType::HandshakeCompleted,
                severity: "info".to_string(),
                source_ip: "10.0.0.1".to_string(),
                api_key_name: None,
                endpoint: "/rfo/handshake".to_string(),
                method: "POST".to_string(),
                status_code: Some(200),
                message: format!("Handshake #{}", i),
                metadata: None,
            });
        }

        let events = logger.recent_events(3);
        assert_eq!(events.len(), 3);
    }
}
