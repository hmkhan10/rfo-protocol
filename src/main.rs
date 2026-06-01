use clap::Parser;
use rfo_core::cli::Cli;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let has_subcommand = args.len() > 1
        && matches!(
            args[1].as_str(),
            "compile" | "watch" | "serve" | "inspect" | "audit" | "help" | "--help" | "-h"
        );

    if has_subcommand {
        let cli = Cli::parse();
        rfo_core::cli::execute(cli).await?;
    } else {
        run_server().await?;
    }

    Ok(())
}

async fn run_server() -> Result<(), Box<dyn std::error::Error>> {
    use axum::routing::{get, post};
    use axum::Router;
    use sqlx::postgres::PgPoolOptions;
    use tower_http::trace::TraceLayer;
    use tracing_subscriber::{fmt, EnvFilter};

    use rfo_core::audit::{AuditEvent, AuditEventType, AuditLogger, DdosProtection};
    use rfo_core::auth::{ApiKeyStore, api_key_middleware};
    use rfo_core::cache;
    use rfo_core::protocol::ProtocolVersion;
    use rfo_core::server::handlers::AppState;
    use rfo_core::server::middleware::{rate_limit_middleware, RateLimitState};
    use rfo_core::server::websocket::WsManager;
    use rfo_core::telemetry::TelemetryTracker;

    // ── 1. Initialize tracing ──────────────────────────────────────────
    fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    tracing::info!("RFO Core Engine v{} starting up", env!("CARGO_PKG_VERSION"));

    // ── 2. Validate secret key ─────────────────────────────────────────
    match std::env::var("RFO_SECRET_KEY") {
        Ok(key) if !key.is_empty() => {
            tracing::info!("RFO_SECRET_KEY loaded ({} bytes)", key.len());
        }
        _ => {
            tracing::error!("RFO_SECRET_KEY environment variable is not set or empty");
            panic!("RFO_SECRET_KEY must be set in the environment");
        }
    }

    // ── 3. Database connection pool ────────────────────────────────────
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://localhost/rfo_protocol".to_string());

    tracing::info!("Connecting to database...");

    let pool = PgPoolOptions::new()
        .max_connections(20)
        .min_connections(2)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .connect(&database_url)
        .await
        .expect("Failed to connect to PostgreSQL database");

    tracing::info!("Database connection pool established");

    // ── 4. Run migrations ──────────────────────────────────────────────
    tracing::info!("Running database migrations...");
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("Failed to run database migrations");
    tracing::info!("Migrations complete");

    // ── 5. Initialize security subsystems ───────────────────────────────
    let api_keys = ApiKeyStore::from_env();
    tracing::info!("API keys loaded: {}", api_keys.len());

    let ddos = DdosProtection::new(
        std::env::var("RFO_DDOSS_MAX_PER_IP")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(100),
        std::env::var("RFO_DDOSS_MAX_GLOBAL")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1000),
    );

    let audit = AuditLogger::with_ddos_protection(Some(pool.clone()), ddos.clone());

    // ── 6. Initialize core subsystems ───────────────────────────────────
    let cache = cache::RfoCache::new();
    let rate_limit = RateLimitState::new();
    let telemetry = TelemetryTracker::new();

    // Background cache cleanup (every 60s)
    let cache_cleanup = cache.clone();
    let telemetry_cleanup = telemetry.clone();
    let audit_cleanup = audit.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
        loop {
            interval.tick().await;
            let before = cache_cleanup.len();
            cache_cleanup.cleanup_expired();
            let after = cache_cleanup.len();
            let expired = (before as u64).saturating_sub(after as u64);
            if expired > 0 {
                telemetry_cleanup.record_expired(expired);
                audit_cleanup.log(AuditEvent {
                    event_type: AuditEventType::ServerError,
                    severity: "info".to_string(),
                    source_ip: "internal".to_string(),
                    api_key_name: None,
                    endpoint: "/internal/cache".to_string(),
                    method: "CLEANUP".to_string(),
                    status_code: None,
                    message: format!("Cache cleanup: {} expired, {} remaining", expired, after),
                    metadata: None,
                });
                tracing::info!("Cache cleanup: {} entries expired, {} remaining", expired, after);
            }
        }
    });

    // Background rate limiter cleanup (every 120s)
    let rate_limit_cleanup = rate_limit.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(120));
        loop {
            interval.tick().await;
            rate_limit_cleanup.cleanup();
        }
    });

    // Log server start
    audit.log(AuditEvent {
        event_type: AuditEventType::ServerStart,
        severity: "info".to_string(),
        source_ip: "internal".to_string(),
        api_key_name: None,
        endpoint: "/internal".to_string(),
        method: "START".to_string(),
        status_code: None,
        message: format!("RFO Engine v{} starting", env!("CARGO_PKG_VERSION")),
        metadata: None,
    });

    // ── 7. Build application state ─────────────────────────────────────
    let ws_manager = WsManager::new();

    let state = AppState {
        db: pool.clone(),
        cache: cache.clone(),
        rate_limit: rate_limit.clone(),
        telemetry: telemetry.clone(),
        api_keys: api_keys.clone(),
        audit: audit.clone(),
        ddos: ddos.clone(),
        ws_manager: ws_manager.clone(),
    };

    let admin_state = rfo_core::admin::AdminState {
        db: pool,
        api_keys: api_keys.clone(),
        cache,
        telemetry,
    };

    // ── 8. Build Axum router ──────────────────────────────────────────
    let admin_routes = Router::new()
        .route("/login", post(rfo_core::admin::admin_login))
        .route("/users", post(rfo_core::admin::create_admin_user))
        .route("/users/{id}/password", axum::routing::put(rfo_core::admin::change_password))
        .route("/stats", get(rfo_core::admin::get_system_stats))
        .route("/sites", get(rfo_core::admin::list_admin_sites))
        .route("/sites/{domain}", axum::routing::delete(rfo_core::admin::delete_site))
        .route("/audit", get(rfo_core::admin::list_audit_logs))
        .route("/keys", get(rfo_core::admin::list_api_keys))
        .route("/keys", post(rfo_core::admin::create_api_key))
        .route("/keys/{name}", axum::routing::delete(rfo_core::admin::revoke_api_key))
        .route("/cache/purge", post(rfo_core::admin::purge_cache))
        .route("/health", get(rfo_core::admin::admin_health))
        .with_state(admin_state);

    let app = Router::new()
        // Public endpoints (no API key required)
        .route("/rfo/health", get(rfo_core::server::handlers::health_check))
        .route("/rfo/capabilities", get(rfo_core::server::handlers::capabilities))
        .route("/rfo/negotiate", post(rfo_core::server::handlers::negotiate))
        // Protected endpoints (API key required)
        .route("/rfo/handshake", post(rfo_core::server::handlers::handshake))
        .route("/rfo/batch-handshake", post(rfo_core::server::handlers::batch_handshake))
        .route("/rfo/doc/{domain}", get(rfo_core::server::handlers::get_doc))
        .route("/rfo/mdoc/{domain}", get(rfo_core::server::handlers::get_mdoc))
        .route("/rfo/stream/{domain}", get(rfo_core::server::handlers::stream_doc))
        .route("/rfo/stream-mdoc/{domain}", get(rfo_core::server::handlers::stream_mdoc))
        .route("/rfo/sites", get(rfo_core::server::handlers::list_sites))
        .route("/rfo/telemetry", get(rfo_core::server::handlers::get_telemetry))
        // Admin endpoints (separate state, no API key — admin token auth)
        .nest("/rfo/admin", admin_routes)
        // WebSocket endpoint
        .route("/rfo/ws", get(
            rfo_core::server::websocket::ws_handler,
        ))
        // API key auth on protected routes
        .layer(axum::middleware::from_fn_with_state(
            api_keys.clone(),
            api_key_middleware,
        ))
        // Rate limiting
        .layer(axum::middleware::from_fn_with_state(
            rate_limit.clone(),
            rate_limit_middleware,
        ))
        // Timeout (2s)
        .layer(tower_http::timeout::TimeoutLayer::with_status_code(
            axum::http::StatusCode::REQUEST_TIMEOUT,
            std::time::Duration::from_secs(2),
        ))
        // Body size limit (2MB)
        .layer(tower_http::limit::RequestBodyLimitLayer::new(2 * 1024 * 1024))
        // CORS layer
        .layer(rfo_core::audit::build_cors())
        // Request tracing
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    // ── 9. Bind and serve ──────────────────────────────────────────────
    let bind_addr = std::env::var("RFO_BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:3000".to_string());
    let listener = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .unwrap_or_else(|_| panic!("Failed to bind to {}", bind_addr));

    tracing::info!("RFO Core Engine listening on {}", bind_addr);
    tracing::info!("Protocol Version: {}", ProtocolVersion::current());
    tracing::info!("Endpoints:");
    tracing::info!("  POST /rfo/handshake        — Duplex handshake");
    tracing::info!("  POST /rfo/batch-handshake  — Batch handshake (up to 20 domains)");
    tracing::info!("  GET  /rfo/doc/:domain        — Full .doc payload");
    tracing::info!("  GET  /rfo/mdoc/:domain       — Mini .mdoc payload");
    tracing::info!("  GET  /rfo/stream/:domain     — Stream .doc (binary)");
    tracing::info!("  GET  /rfo/stream-mdoc/:domain — Stream .mdoc (binary)");
    tracing::info!("  GET  /rfo/sites             — List registered sites");
    tracing::info!("  GET  /rfo/telemetry         — Telemetry dashboard");
    tracing::info!("  GET  /rfo/capabilities      — Protocol capabilities");
    tracing::info!("  POST /rfo/negotiate         — Capability negotiation");
    tracing::info!("  GET  /rfo/ws                — WebSocket (real-time updates)");
    tracing::info!("  GET  /rfo/health            — Health check (public)");
    tracing::info!("Admin:");
    tracing::info!("  POST /rfo/admin/login       — Admin login");
    tracing::info!("  POST /rfo/admin/users       — Create admin user");
    tracing::info!("  GET  /rfo/admin/stats       — System statistics");
    tracing::info!("  GET  /rfo/admin/sites       — List sites (paginated)");
    tracing::info!("  DEL  /rfo/admin/sites/:d    — Delete site");
    tracing::info!("  GET  /rfo/admin/audit       — Audit logs (paginated)");
    tracing::info!("  GET  /rfo/admin/keys        — List API keys");
    tracing::info!("  POST /rfo/admin/keys        — Create API key");
    tracing::info!("  DEL  /rfo/admin/keys/:name  — Revoke API key");
    tracing::info!("  POST /rfo/admin/cache/purge — Purge cache");
    tracing::info!("  GET  /rfo/admin/health      — Detailed health");
    tracing::info!("Security:");
    tracing::info!("  API Key: X-API-Key header required for /rfo/* endpoints");
    tracing::info!("  CORS: Origins from RFO_CORS_ORIGINS env var");
    tracing::info!("  DDoS: max {} req/min per IP, {} global", ddos.active_connections(), 1000);

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await
    .expect("Server failed");

    Ok(())
}
