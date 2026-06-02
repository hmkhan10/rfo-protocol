use std::collections::HashMap;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use chrono::Utc;
use sqlx::PgPool;
use sqlx::Row;

use crate::audit::{AuditLogger, DdosProtection};
use crate::auth::ApiKeyStore;
use crate::cache::RfoCache;
use crate::compiler::{calculate_quality_score, compile_doc, compile_mdoc};
use crate::crypto::site_id as crypto;
use crate::parser::{extract_coordinates, parse_html, parse_markdown};
use crate::protocol::{PayloadEncoding, ProtocolVersion, PROTOCOL_VERSION};
use crate::rfo_protocol::{
    CacheEntry, FullDocPayload, HandshakeRequest, HandshakeResponse, MiniDocPayload, Payload,
    PayloadType, RfoHeader,
};
use crate::server::middleware::RateLimitState;
use crate::server::websocket::WsManager;
use crate::telemetry::{RequestTelemetry, TelemetryTracker};

// ── Application State ──────────────────────────────────────────────────────

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub cache: RfoCache,
    pub rate_limit: RateLimitState,
    pub telemetry: TelemetryTracker,
    pub api_keys: ApiKeyStore,
    pub audit: AuditLogger,
    pub ddos: DdosProtection,
    pub ws_manager: WsManager,
}

// ── Health Check ───────────────────────────────────────────────────────────

pub async fn health_check() -> Json<serde_json::Value> {
    let version = ProtocolVersion::current();
    Json(serde_json::json!({
        "status": "healthy",
        "protocol": "rfo-core",
        "version": env!("CARGO_PKG_VERSION"),
        "protocol_version": version.to_string(),
        "min_supported": crate::protocol::MIN_SUPPORTED_VERSION,
        "capabilities": ["json", "msgpack", "websocket", "streaming"]
    }))
}

// ── GET /rfo/capabilities ──────────────────────────────────────────────────

pub async fn capabilities() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "protocol_version": PROTOCOL_VERSION,
        "encodings": ["application/json", "application/msgpack"],
        "features": [
            "handshake",
            "batch-handshake",
            "websocket",
            "streaming",
            "capability-negotiation",
            "rate-limiting",
            "api-key-auth",
            "cors",
            "audit-logging"
        ],
        "max_batch_size": 20,
        "max_payload_size_bytes": 2097152,
        "websocket": {
            "max_connections": 1000,
            "ping_interval_ms": 30000
        }
    }))
}

// ── POST /rfo/negotiate ────────────────────────────────────────────────────

pub async fn negotiate(
    Json(req): Json<crate::protocol::CapabilityRequest>,
) -> Json<crate::protocol::CapabilityResponse> {
    let server_features = vec![
        "handshake".to_string(),
        "batch-handshake".to_string(),
        "websocket".to_string(),
        "streaming".to_string(),
    ];

    let negotiated = if req.supported_encodings.contains(&PayloadEncoding::MessagePack) {
        PayloadEncoding::MessagePack
    } else {
        PayloadEncoding::Json
    };

    let common_features: Vec<String> = req
        .supported_features
        .iter()
        .filter(|f| server_features.contains(f))
        .cloned()
        .collect();

    Json(crate::protocol::CapabilityResponse {
        negotiated_encoding: negotiated.clone(),
        supported_features: common_features,
        protocol_version: PROTOCOL_VERSION.to_string(),
        server_capabilities: server_features,
    })
}

// ── GET /rfo/stream/:domain ────────────────────────────────────────────────

pub async fn stream_doc(
    State(state): State<AppState>,
    Path(domain): Path<String>,
) -> Result<(axum::http::header::HeaderMap, Vec<u8>), (StatusCode, String)> {
    if let Some(entry) = state.cache.get_by_domain(&domain) {
        state.telemetry.record_cache_hit();

        // Serialize based on accept header (simplified - always JSON for now)
        let payload = serde_json::to_vec(&entry.doc)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        let mut headers = axum::http::header::HeaderMap::new();
        headers.insert(
            axum::http::header::CONTENT_TYPE,
            "application/json".parse().unwrap(),
        );
        headers.insert(
            axum::http::header::CONTENT_LENGTH,
            payload.len().to_string().parse().unwrap(),
        );
        headers.insert("X-RFO-Protocol-Version", PROTOCOL_VERSION.parse().unwrap());
        headers.insert("X-RFO-Site-ID", entry.header.site_id.parse().unwrap());

        return Ok((headers, payload));
    }

    state.telemetry.record_cache_miss();
    Err((
        StatusCode::NOT_FOUND,
        format!("No .doc compiled for '{}'. Run a handshake first.", domain),
    ))
}

// ── GET /rfo/stream-mdoc/:domain ───────────────────────────────────────────

pub async fn stream_mdoc(
    State(state): State<AppState>,
    Path(domain): Path<String>,
) -> Result<(axum::http::header::HeaderMap, Vec<u8>), (StatusCode, String)> {
    if let Some(entry) = state.cache.get_by_domain(&domain) {
        state.telemetry.record_cache_hit();

        let payload = serde_json::to_vec(&entry.mdoc)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        let mut headers = axum::http::header::HeaderMap::new();
        headers.insert(
            axum::http::header::CONTENT_TYPE,
            "application/json".parse().unwrap(),
        );
        headers.insert(
            axum::http::header::CONTENT_LENGTH,
            payload.len().to_string().parse().unwrap(),
        );
        headers.insert("X-RFO-Protocol-Version", PROTOCOL_VERSION.parse().unwrap());
        headers.insert("X-RFO-Site-ID", entry.header.site_id.parse().unwrap());

        return Ok((headers, payload));
    }

    state.telemetry.record_cache_miss();
    Err((
        StatusCode::NOT_FOUND,
        format!("No .mdoc compiled for '{}'. Run a handshake first.", domain),
    ))
}

// ── POST /rfo/handshake ────────────────────────────────────────────────────

pub async fn handshake(
    State(state): State<AppState>,
    Json(request): Json<HandshakeRequest>,
) -> Result<Json<HandshakeResponse>, (StatusCode, String)> {
    let start = std::time::Instant::now();
    let now = Utc::now();

    // 1. Verify nonce freshness (replay protection: 5-minute window)
    if !crypto::verify_handshake_nonce(&request.nonce, request.timestamp) {
        state.telemetry.record_cache_miss();
        state.telemetry.record_request(RequestTelemetry {
            domain: request.domain_url.clone(),
            cache_hit: false,
            processing_time_ms: 0,
            payload_type: format!("{:?}", request.requested_payload),
            timestamp: now,
        });
        return Err((
            StatusCode::UNAUTHORIZED,
            "Invalid or expired nonce".to_string(),
        ));
    }

    // 2. Check cache first (sub-5ms path)
    if let Some(entry) = state.cache.get(&request.domain_url) {
        state.telemetry.record_cache_hit();

        let processing_time = start.elapsed().as_millis() as u64;
        let response = HandshakeResponse {
            header: entry.header.clone(),
            payload: match request.requested_payload {
                PayloadType::Doc => Payload::Doc(entry.doc.clone()),
                PayloadType::Mdoc => Payload::Mdoc(entry.mdoc.clone()),
            },
            processing_time_ms: processing_time,
            nonce: request.nonce.clone(),
        };

        state.telemetry.record_handshake(&entry.header.site_id);
        state.telemetry.record_request(RequestTelemetry {
            domain: request.domain_url.clone(),
            cache_hit: true,
            processing_time_ms: processing_time,
            payload_type: format!("{:?}", request.requested_payload),
            timestamp: now,
        });

        // Log telemetry to DB
        log_handshake(&state.db, &entry.header.site_id, &request.nonce, &now, processing_time, 200).await;

        return Ok(Json(response));
    }

    state.telemetry.record_cache_miss();

    // 3. Fetch the target URL content
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let html = client
        .get(&request.domain_url)
        .send()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Failed to fetch URL: {}", e)))?
        .text()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Failed to read response: {}", e)))?;

    // 4. Parse content
    let parsed = if request.domain_url.ends_with(".md") {
        parse_markdown(&html)
    } else {
        parse_html(&html)
    };

    // 5. Extract coordinates (merge with client-provided)
    let mut coordinates = extract_coordinates(&parsed);
    coordinates.extend(request.coordinates.clone());

    // 6. Generate site_id
    let site_id = crypto::generate_site_id(&request.domain_url)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // 7. Compile payloads
    let mdoc = compile_mdoc(&parsed);
    let doc = compile_doc(&parsed, &request.domain_url);
    let quality_score = calculate_quality_score(&mdoc, &doc);

    let header = RfoHeader::new(site_id.clone(), coordinates.clone(), quality_score);

    // 8. Cache the compiled entry
    let entry = CacheEntry {
        header: header.clone(),
        doc: doc.clone(),
        mdoc: mdoc.clone(),
        cached_at: Utc::now(),
    };
    state.cache.insert(request.domain_url.clone(), entry);

    // 9. Upsert site record in database
    upsert_site(&state.db, &site_id, &request.domain_url, quality_score, &coordinates).await;

    // 10. Record telemetry
    state.telemetry.record_quality_score(&site_id, &request.domain_url, quality_score);
    state.telemetry.record_handshake(&site_id);

    // 11. Build response
    let processing_time = start.elapsed().as_millis() as u64;
    let payload = match request.requested_payload {
        PayloadType::Doc => Payload::Doc(doc),
        PayloadType::Mdoc => Payload::Mdoc(mdoc),
    };

    let response = HandshakeResponse {
        header,
        payload,
        processing_time_ms: processing_time,
        nonce: request.nonce.clone(),
    };

    state.telemetry.record_request(RequestTelemetry {
        domain: request.domain_url.clone(),
        cache_hit: false,
        processing_time_ms: processing_time,
        payload_type: format!("{:?}", request.requested_payload),
        timestamp: now,
    });

    // 12. Log telemetry to DB
    log_handshake(&state.db, &site_id, &request.nonce, &now, processing_time, 200).await;

    Ok(Json(response))
}

// ── POST /rfo/batch-handshake ──────────────────────────────────────────────

#[derive(Debug, serde::Deserialize)]
pub struct BatchHandshakeRequest {
    pub domains: Vec<String>,
    #[serde(default)]
    pub coordinates: HashMap<String, String>,
    #[serde(default = "default_payload_type")]
    pub requested_payload: PayloadType,
}

fn default_payload_type() -> PayloadType {
    PayloadType::Mdoc
}

pub async fn batch_handshake(
    State(state): State<AppState>,
    Json(request): Json<BatchHandshakeRequest>,
) -> Result<Json<Vec<serde_json::Value>>, (StatusCode, String)> {
    if request.domains.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "domains list cannot be empty".to_string()));
    }

    if request.domains.len() > 20 {
        return Err((StatusCode::BAD_REQUEST, "max 20 domains per batch".to_string()));
    }

    let start = std::time::Instant::now();
    let now = Utc::now();
    let nonce = crypto::generate_handshake_nonce();
    let timestamp = now.timestamp();

    let mut handles = Vec::with_capacity(request.domains.len());

    for domain in &request.domains {
        let state_clone = state.clone();
        let domain = domain.clone();
        let coords = request.coordinates.clone();
        let ptype = request.requested_payload.clone();
        let nonce_inner = nonce.clone();
        let ts = timestamp;

        handles.push(tokio::spawn(async move {
            let req = HandshakeRequest {
                domain_url: domain.clone(),
                coordinates: coords,
                requested_payload: ptype,
                nonce: nonce_inner,
                timestamp: ts,
            };

            match process_handshake_inner(&state_clone, req).await {
                Ok(resp) => serde_json::json!({
                    "domain": domain,
                    "status": "ok",
                    "site_id": resp.header.site_id,
                    "quality_score": resp.header.quality_score,
                    "processing_time_ms": resp.processing_time_ms,
                }),
                Err((status, msg)) => serde_json::json!({
                    "domain": domain,
                    "status": "error",
                    "error": msg,
                    "status_code": status.as_u16(),
                }),
            }
        }));
    }

    let mut results = Vec::with_capacity(handles.len());
    for handle in handles {
        match handle.await {
            Ok(result) => results.push(result),
            Err(e) => {
                results.push(serde_json::json!({
                    "status": "error",
                    "error": format!("Task panicked: {}", e),
                }));
            }
        }
    }

    let batch_time = start.elapsed().as_millis() as u64;
    tracing::info!(
        "Batch handshake completed: {} domains in {}ms",
        request.domains.len(),
        batch_time
    );

    Ok(Json(results))
}

// ── GET /rfo/telemetry ─────────────────────────────────────────────────────

pub async fn get_telemetry(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let cache_entries = state.cache.len();
    let report = state.telemetry.report(cache_entries);
    Json(serde_json::to_value(report).unwrap_or_default())
}

// ── GET /rfo/doc/:domain ───────────────────────────────────────────────────

pub async fn get_doc(
    State(state): State<AppState>,
    Path(domain): Path<String>,
) -> Result<Json<FullDocPayload>, (StatusCode, String)> {
    if let Some(entry) = state.cache.get_by_domain(&domain) {
        state.telemetry.record_cache_hit();
        return Ok(Json(entry.doc));
    }

    state.telemetry.record_cache_miss();
    Err((
        StatusCode::NOT_FOUND,
        format!("No .doc compiled for '{}'. Run a handshake first.", domain),
    ))
}

// ── GET /rfo/mdoc/:domain ──────────────────────────────────────────────────

pub async fn get_mdoc(
    State(state): State<AppState>,
    Path(domain): Path<String>,
) -> Result<Json<MiniDocPayload>, (StatusCode, String)> {
    if let Some(entry) = state.cache.get_by_domain(&domain) {
        state.telemetry.record_cache_hit();
        return Ok(Json(entry.mdoc));
    }

    state.telemetry.record_cache_miss();
    Err((
        StatusCode::NOT_FOUND,
        format!("No .mdoc compiled for '{}'. Run a handshake first.", domain),
    ))
}

// ── GET /rfo/sites ─────────────────────────────────────────────────────────

pub async fn list_sites(
    State(state): State<AppState>,
) -> Result<Json<Vec<serde_json::Value>>, (StatusCode, String)> {
    let raw_rows = sqlx::query(
        r#"SELECT site_id, domain_url, quality_score, coordinates, created_at, updated_at
           FROM sites ORDER BY quality_score DESC LIMIT 50"#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut sites = Vec::new();
    for row in raw_rows {
        let site_id: String = row.get("site_id");
        let domain_url: String = row.get("domain_url");
        let quality_score: i32 = row.get("quality_score");
        let coordinates: serde_json::Value = row.get("coordinates");
        let created_at: chrono::DateTime<Utc> = row.get("created_at");
        let updated_at: chrono::DateTime<Utc> = row.get("updated_at");

        sites.push(serde_json::json!({
            "site_id": site_id,
            "domain_url": domain_url,
            "quality_score": quality_score,
            "coordinates": coordinates,
            "created_at": created_at,
            "updated_at": updated_at,
        }));
    }

    Ok(Json(sites))
}

// ── Inner handshake processor (for batch reuse) ────────────────────────────

async fn process_handshake_inner(
    state: &AppState,
    request: HandshakeRequest,
) -> Result<HandshakeResponse, (StatusCode, String)> {
    let start = std::time::Instant::now();
    let now = Utc::now();

    if !crypto::verify_handshake_nonce(&request.nonce, request.timestamp) {
        return Err((StatusCode::UNAUTHORIZED, "Invalid nonce".to_string()));
    }

    // Check cache
    if let Some(entry) = state.cache.get(&request.domain_url) {
        state.telemetry.record_cache_hit();
        let processing_time = start.elapsed().as_millis() as u64;
        let response = HandshakeResponse {
            header: entry.header.clone(),
            payload: match request.requested_payload {
                PayloadType::Doc => Payload::Doc(entry.doc.clone()),
                PayloadType::Mdoc => Payload::Mdoc(entry.mdoc.clone()),
            },
            processing_time_ms: processing_time,
            nonce: request.nonce.clone(),
        };
        state.telemetry.record_handshake(&entry.header.site_id);
        log_handshake(&state.db, &entry.header.site_id, &request.nonce, &now, processing_time, 200).await;
        return Ok(response);
    }

    state.telemetry.record_cache_miss();

    // Fetch + parse + compile
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let html = client
        .get(&request.domain_url)
        .send()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Fetch failed: {}", e)))?
        .text()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Read failed: {}", e)))?;

    let parsed = if request.domain_url.ends_with(".md") {
        parse_markdown(&html)
    } else {
        parse_html(&html)
    };

    let mut coordinates = extract_coordinates(&parsed);
    coordinates.extend(request.coordinates.clone());

    let site_id = crypto::generate_site_id(&request.domain_url)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mdoc = compile_mdoc(&parsed);
    let doc = compile_doc(&parsed, &request.domain_url);
    let quality_score = calculate_quality_score(&mdoc, &doc);
    let header = RfoHeader::new(site_id.clone(), coordinates.clone(), quality_score);

    let entry = CacheEntry {
        header: header.clone(),
        doc: doc.clone(),
        mdoc: mdoc.clone(),
        cached_at: Utc::now(),
    };
    state.cache.insert(request.domain_url.clone(), entry);
    upsert_site(&state.db, &site_id, &request.domain_url, quality_score, &coordinates).await;

    state.telemetry.record_quality_score(&site_id, &request.domain_url, quality_score);
    state.telemetry.record_handshake(&site_id);

    let processing_time = start.elapsed().as_millis() as u64;
    let payload = match request.requested_payload {
        PayloadType::Doc => Payload::Doc(doc),
        PayloadType::Mdoc => Payload::Mdoc(mdoc),
    };

    log_handshake(&state.db, &site_id, &request.nonce, &now, processing_time, 200).await;

    Ok(HandshakeResponse {
        header,
        payload,
        processing_time_ms: processing_time,
        nonce: request.nonce,
    })
}

// ── Database Helpers ───────────────────────────────────────────────────────

async fn upsert_site(
    pool: &PgPool,
    site_id: &str,
    domain_url: &str,
    quality_score: u32,
    coordinates: &HashMap<String, String>,
) {
    let coords_json = serde_json::to_value(coordinates).unwrap_or(serde_json::json!({}));

    let result = sqlx::query(
        r#"INSERT INTO sites (site_id, domain_url, quality_score, coordinates)
           VALUES ($1, $2, $3, $4)
           ON CONFLICT (site_id) DO UPDATE SET
               quality_score = EXCLUDED.quality_score,
               coordinates = EXCLUDED.coordinates,
               updated_at = CURRENT_TIMESTAMP"#,
    )
    .bind(site_id)
    .bind(domain_url)
    .bind(quality_score as i32)
    .bind(coords_json)
    .execute(pool)
    .await;

    if let Err(e) = result {
        tracing::error!("Failed to upsert site {}: {}", site_id, e);
    }
}

async fn log_handshake(
    pool: &PgPool,
    site_id: &str,
    nonce: &str,
    timestamp: &chrono::DateTime<Utc>,
    processing_time_ms: u64,
    status_code: i32,
) {
    let result = sqlx::query(
        r#"INSERT INTO handshake_logs (site_id, request_timestamp, nonce, processing_time_ms, client_ip, status_code)
           VALUES ($1, $2, $3, $4, 'internal', $5)"#,
    )
    .bind(site_id)
    .bind(timestamp)
    .bind(nonce)
    .bind(processing_time_ms as i32)
    .bind(status_code)
    .execute(pool)
    .await;

    if let Err(e) = result {
        tracing::error!("Failed to log handshake: {}", e);
    }
}
