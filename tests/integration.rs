// ─────────────────────────────────────────────────────────────────────────────
// RFO Protocol — Integration Tests
// ─────────────────────────────────────────────────────────────────────────────
// Tests the full HTTP stack using tower::ServiceExt::oneshot.
// Replicates the production middleware stack.
// ─────────────────────────────────────────────────────────────────────────────

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use axum::Router;
use tower::ServiceExt;

use rfo_core::audit::{AuditLogger, DdosProtection};
use rfo_core::auth::{ApiKeyStore, api_key_middleware};
use rfo_core::cache;
use rfo_core::server::handlers::AppState;
use rfo_core::server::middleware::RateLimitState;
use rfo_core::server::websocket::WsManager;
use rfo_core::telemetry::TelemetryTracker;

fn setup() {
    std::env::set_var("RFO_SECRET_KEY", "integration-test-secret-key-32bytes!");
    std::env::set_var(
        "RFO_API_KEYS",
        "agent_alpha:integration_key_alpha,agent_beta:integration_key_beta",
    );
}

fn build_app() -> (Router, ApiKeyStore) {
    setup();

    let api_keys = ApiKeyStore::from_env();
    let ddos = DdosProtection::new(100, 1000);
    let rate_limit = RateLimitState::new();

    let state = AppState {
        db: {
            let url = std::env::var("DATABASE_URL")
                .unwrap_or_else(|_| "postgres://localhost/rfo_protocol_test".to_string());
            sqlx::postgres::PgPoolOptions::new()
                .max_connections(1)
                .acquire_timeout(std::time::Duration::from_secs(2))
                .connect_lazy(&url)
                .expect("Failed to create lazy pool")
        },
        cache: cache::RfoCache::new(),
        rate_limit: rate_limit.clone(),
        telemetry: TelemetryTracker::new(),
        api_keys: api_keys.clone(),
        audit: AuditLogger::with_ddos_protection(None, ddos.clone()),
        ddos,
        ws_manager: WsManager::new(),
    };

    let app = Router::new()
        // Public endpoints
        .route("/rfo/health", get(rfo_core::server::handlers::health_check))
        .route("/rfo/capabilities", get(rfo_core::server::handlers::capabilities))
        .route("/rfo/negotiate", axum::routing::post(rfo_core::server::handlers::negotiate))
        // Protected endpoints
        .route("/rfo/batch-handshake", axum::routing::post(rfo_core::server::handlers::batch_handshake))
        .route("/rfo/sites", get(rfo_core::server::handlers::list_sites))
        .route("/rfo/telemetry", get(rfo_core::server::handlers::get_telemetry))
        // API key auth middleware
        .layer(axum::middleware::from_fn_with_state(
            api_keys.clone(),
            api_key_middleware,
        ))
        .with_state(state);

    (app, api_keys)
}

// ── Health Check ────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_health_check_returns_200() {
    let (app, _) = build_app();
    let response = app
        .oneshot(Request::builder().uri("/rfo/health").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "healthy");
    assert_eq!(json["protocol"], "rfo-core");
    assert!(json["version"].is_string());
    assert!(json["protocol_version"].is_string());
    assert!(json["capabilities"].is_array());
}

#[tokio::test]
async fn test_health_no_auth_required() {
    let (app, _) = build_app();
    let response = app
        .oneshot(Request::builder().uri("/rfo/health").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

// ── Capabilities (public) ───────────────────────────────────────────────────

#[tokio::test]
async fn test_capabilities_returns_200() {
    let (app, _) = build_app();
    let response = app
        .oneshot(Request::builder().uri("/rfo/capabilities").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["encodings"].is_array());
    assert!(json["features"].is_array());
    assert_eq!(json["max_batch_size"], 20);
    assert_eq!(json["max_payload_size_bytes"], 2097152);
}

#[tokio::test]
async fn test_capabilities_has_required_features() {
    let (app, _) = build_app();
    let response = app
        .oneshot(Request::builder().uri("/rfo/capabilities").body(Body::empty()).unwrap())
        .await
        .unwrap();

    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let features = json["features"].as_array().unwrap();
    for r in &["handshake", "batch-handshake", "websocket", "streaming"] {
        assert!(features.contains(&serde_json::json!(r)), "Missing feature: {}", r);
    }
}

// ── Negotiate (public) ─────────────────────────────────────────────────────

#[tokio::test]
async fn test_negotiate_json_encoding() {
    let (app, _) = build_app();
    let body = serde_json::json!({
        "supported_encodings": ["application/json"],
        "supported_features": ["handshake", "websocket"],
        "protocol_version": "1.0.0"
    });
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/rfo/negotiate")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let resp_body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&resp_body).unwrap();
    assert_eq!(json["negotiated_encoding"], "application/json");
    assert!(json["supported_features"].as_array().unwrap().contains(&serde_json::json!("handshake")));
}

#[tokio::test]
async fn test_negotiate_msgpack_preferred() {
    let (app, _) = build_app();
    let body = serde_json::json!({
        "supported_encodings": ["application/msgpack", "application/json"],
        "supported_features": ["handshake"],
        "protocol_version": "1.0.0"
    });
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/rfo/negotiate")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    let resp_body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&resp_body).unwrap();
    assert_eq!(json["negotiated_encoding"], "application/msgpack");
}

#[tokio::test]
async fn test_negotiate_no_matching_features() {
    let (app, _) = build_app();
    let body = serde_json::json!({
        "supported_encodings": ["application/json"],
        "supported_features": ["nonexistent-feature"],
        "protocol_version": "1.0.0"
    });
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/rfo/negotiate")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    let resp_body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&resp_body).unwrap();
    assert!(json["supported_features"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_negotiate_returns_server_capabilities() {
    let (app, _) = build_app();
    let body = serde_json::json!({
        "supported_encodings": ["application/json"],
        "supported_features": [],
        "protocol_version": "1.0.0"
    });
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/rfo/negotiate")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    let resp_body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&resp_body).unwrap();
    let server_caps = json["server_capabilities"].as_array().unwrap();
    assert!(!server_caps.is_empty());
    assert!(server_caps.contains(&serde_json::json!("handshake")));
}

// ── Auth: Missing / Invalid Key ─────────────────────────────────────────────

#[tokio::test]
async fn test_missing_api_key_returns_401() {
    let (app, _) = build_app();
    let response = app
        .oneshot(Request::builder().uri("/rfo/sites").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_invalid_api_key_returns_401() {
    let (app, _) = build_app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/rfo/sites")
                .header("x-api-key", "totally-invalid-key")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_valid_api_key_passes_auth() {
    let (app, _) = build_app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/rfo/sites")
                .header("x-api-key", "integration_key_alpha")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    // Should not be 401 — may be 500 (DB not available) but auth passes
    assert_ne!(response.status(), StatusCode::UNAUTHORIZED);
}

// ── Batch Handshake Validation ──────────────────────────────────────────────

#[tokio::test]
async fn test_batch_empty_domains_returns_400() {
    let (app, _) = build_app();
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/rfo/batch-handshake")
                .header("content-type", "application/json")
                .header("x-api-key", "integration_key_alpha")
                .body(Body::from(serde_json::json!({"domains": []}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_batch_too_many_domains_returns_400() {
    let (app, _) = build_app();
    let domains: Vec<String> = (0..21).map(|i| format!("https://{}.com", i)).collect();
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/rfo/batch-handshake")
                .header("content-type", "application/json")
                .header("x-api-key", "integration_key_alpha")
                .body(Body::from(serde_json::json!({"domains": domains}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_batch_malformed_json_returns_error() {
    let (app, _) = build_app();
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/rfo/batch-handshake")
                .header("content-type", "application/json")
                .header("x-api-key", "integration_key_alpha")
                .body(Body::from("{bad json"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(
        response.status() == StatusCode::UNPROCESSABLE_ENTITY
            || response.status() == StatusCode::BAD_REQUEST
    );
}

// ── Unknown Routes ──────────────────────────────────────────────────────────

#[tokio::test]
async fn test_unknown_route_returns_404_or_401() {
    let (app, _) = build_app();
    let response = app
        .oneshot(Request::builder().uri("/rfo/nonexistent").body(Body::empty()).unwrap())
        .await
        .unwrap();
    // Auth middleware may return 401 before router returns 404
    assert!(
        response.status() == StatusCode::NOT_FOUND
            || response.status() == StatusCode::UNAUTHORIZED
    );
}

// ── Protocol Version Format ─────────────────────────────────────────────────

#[tokio::test]
async fn test_health_returns_semver() {
    let (app, _) = build_app();
    let response = app
        .oneshot(Request::builder().uri("/rfo/health").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let version = json["protocol_version"].as_str().unwrap();
    let parts: Vec<&str> = version.split('.').collect();
    assert_eq!(parts.len(), 3);
    assert!(parts[0].parse::<u16>().is_ok());
}
