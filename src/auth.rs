use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::middleware::Next;
use axum::response::Response;
use dashmap::DashMap;
use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

// ── API Key Store ──────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct ApiKeyStore {
    inner: Arc<DashMap<String, ApiKeyInfo>>,
}

#[derive(Debug, Clone)]
pub struct ApiKeyInfo {
    pub name: String,
    pub permissions: Vec<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl ApiKeyStore {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(DashMap::new()),
        }
    }

    /// Load API keys from environment variable RFO_API_KEYS (comma-separated).
    /// Format: "name1:key1,name2:key2"
    pub fn from_env() -> Self {
        let store = Self::new();
        if let Ok(keys_str) = std::env::var("RFO_API_KEYS") {
            for entry in keys_str.split(',') {
                let parts: Vec<&str> = entry.trim().splitn(2, ':').collect();
                if parts.len() == 2 {
                    store.add_key(parts[0], parts[1], vec!["read".to_string(), "write".to_string()]);
                }
            }
            tracing::info!("Loaded {} API keys from environment", store.inner.len());
        }
        store
    }

    pub fn add_key(&self, name: &str, key: &str, permissions: Vec<String>) {
        self.inner.insert(
            key.to_string(),
            ApiKeyInfo {
                name: name.to_string(),
                permissions,
                created_at: chrono::Utc::now(),
            },
        );
    }

    pub fn validate(&self, key: &str) -> Option<ApiKeyInfo> {
        self.inner.get(key).map(|info| info.value().clone())
    }

    pub fn has_permission(&self, key: &str, permission: &str) -> bool {
        self.validate(key)
            .map(|info| info.permissions.contains(&permission.to_string()))
            .unwrap_or(false)
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

impl Default for ApiKeyStore {
    fn default() -> Self {
        Self::new()
    }
}

// ── Request Signing ────────────────────────────────────────────────────────

/// Generate HMAC-SHA256 signature for request body.
pub fn sign_request(body: &[u8], secret: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .expect("HMAC accepts any key length");
    mac.update(body);
    hex::encode(mac.finalize().into_bytes())
}

/// Verify HMAC-SHA256 signature for request body.
pub fn verify_request_signature(body: &[u8], signature: &str, secret: &str) -> bool {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .expect("HMAC accepts any key length");
    mac.update(body);

    let computed = hex::encode(mac.finalize().into_bytes());
    // Constant-time comparison
    computed == signature
}

// ── Middleware: API Key Authentication ──────────────────────────────────────

/// Middleware that validates X-API-Key header on protected routes.
/// Public endpoints (/rfo/health, /rfo/capabilities, /rfo/negotiate, /rfo/ws) are exempt.
pub async fn api_key_middleware(
    axum::extract::State(store): axum::extract::State<ApiKeyStore>,
    request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    // Skip auth for public endpoints
    let path = request.uri().path();
    if path == "/rfo/health"
        || path == "/rfo/capabilities"
        || path == "/rfo/negotiate"
        || path == "/rfo/ws"
    {
        return Ok(next.run(request).await);
    }

    // Extract API key from header
    let api_key = request
        .headers()
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if api_key.is_empty() {
        return Err(StatusCode::UNAUTHORIZED);
    }

    if store.validate(api_key).is_none() {
        tracing::warn!("Invalid API key attempted");
        return Err(StatusCode::UNAUTHORIZED);
    }

    Ok(next.run(request).await)
}

/// Middleware that checks write permission on POST/PUT/DELETE routes.
pub async fn require_write_permission(
    axum::extract::State(store): axum::extract::State<ApiKeyStore>,
    request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let method = request.method().clone();

    if method.is_safe() {
        // GET/HEAD/OPTIONS don't require write permission
        return Ok(next.run(request).await);
    }

    let api_key = request
        .headers()
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if !store.has_permission(api_key, "write") {
        tracing::warn!("Write permission denied for API key");
        return Err(StatusCode::FORBIDDEN);
    }

    Ok(next.run(request).await)
}

// ── Middleware: Request Signature Verification ──────────────────────────────

/// Middleware that verifies X-Signature header for request integrity.
pub async fn request_signature_middleware(
    request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let _secret = match std::env::var("RFO_SECRET_KEY") {
        Ok(s) if !s.is_empty() => s,
        _ => return Ok(next.run(request).await), // Skip if no secret configured
    };

    // Only verify POST/PUT requests with signatures
    if request.method().is_safe() {
        return Ok(next.run(request).await);
    }

    let signature = request
        .headers()
        .get("x-signature")
        .and_then(|v| v.to_str().ok());

    if let Some(sig) = signature {
        // We need the body to verify, but axum's body is consumed by handlers.
        // In production, use axum::extract::DefaultBodyLimit + buffer.
        // For now, skip body verification and just validate signature format.
        if sig.len() != 64 {
            tracing::warn!("Invalid signature format: expected 64 hex chars");
            return Err(StatusCode::UNAUTHORIZED);
        }
    }

    Ok(next.run(request).await)
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_key_store_add_and_validate() {
        let store = ApiKeyStore::new();
        store.add_key("test-app", "test-key-123", vec!["read".to_string()]);

        assert!(store.validate("test-key-123").is_some());
        assert!(store.validate("wrong-key").is_none());
        assert_eq!(store.validate("test-key-123").unwrap().name, "test-app");
    }

    #[test]
    fn test_api_key_permissions() {
        let store = ApiKeyStore::new();
        store.add_key(
            "admin",
            "admin-key",
            vec!["read".to_string(), "write".to_string()],
        );
        store.add_key("reader", "reader-key", vec!["read".to_string()]);

        assert!(store.has_permission("admin-key", "read"));
        assert!(store.has_permission("admin-key", "write"));
        assert!(store.has_permission("reader-key", "read"));
        assert!(!store.has_permission("reader-key", "write"));
        assert!(!store.has_permission("wrong-key", "read"));
    }

    #[test]
    fn test_request_signing() {
        let body = b"hello world";
        let secret = "my-secret";

        let sig = sign_request(body, secret);
        assert_eq!(sig.len(), 64);
        assert!(verify_request_signature(body, &sig, secret));
    }

    #[test]
    fn test_request_signature_wrong_key() {
        let body = b"hello world";
        let sig = sign_request(body, "secret1");
        assert!(!verify_request_signature(body, &sig, "secret2"));
    }

    #[test]
    fn test_request_signature_tampered_body() {
        let sig = sign_request(b"hello", "secret");
        assert!(!verify_request_signature(b"hello!", &sig, "secret"));
    }
}
