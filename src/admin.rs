use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use sqlx::PgPool;
use uuid::Uuid;

use crate::auth::ApiKeyStore;
use crate::cache::RfoCache;
use crate::telemetry::TelemetryTracker;

// ── Types ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminUser {
    pub id: Uuid,
    pub username: String,
    pub role: String,
    pub permissions: serde_json::Value,
    pub last_login: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct AdminLoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct AdminLoginResponse {
    pub token: String,
    pub user: AdminUser,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateAdminRequest {
    pub username: String,
    pub password: String,
    pub role: Option<String>,
    pub permissions: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
pub struct SiteRecord {
    pub id: Uuid,
    pub site_id: String,
    pub domain_url: String,
    pub quality_score: i32,
    pub coordinates: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct SiteQuery {
    pub page: Option<i64>,
    pub per_page: Option<i64>,
    pub sort: Option<String>,
    pub order: Option<String>,
    pub search: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PaginatedResponse<T> {
    pub data: Vec<T>,
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
    pub total_pages: i64,
}

#[derive(Debug, Serialize)]
pub struct AuditLogRecord {
    pub id: Uuid,
    pub event_type: String,
    pub severity: String,
    pub source_ip: Option<String>,
    pub api_key_name: Option<String>,
    pub endpoint: Option<String>,
    pub method: Option<String>,
    pub status_code: Option<i32>,
    pub message: Option<String>,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct AuditQuery {
    pub page: Option<i64>,
    pub per_page: Option<i64>,
    pub event_type: Option<String>,
    pub severity: Option<String>,
    pub source_ip: Option<String>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub struct SystemStats {
    pub total_sites: i64,
    pub total_handshakes: i64,
    pub total_audit_events: i64,
    pub active_api_keys: i64,
    pub cache_entries: usize,
    pub cache_hit_rate: f64,
    pub avg_quality_score: f64,
    pub uptime_seconds: u64,
}

#[derive(Debug, Serialize)]
pub struct ApiKeyRecord {
    pub id: Uuid,
    pub name: String,
    pub key_prefix: String,
    pub permissions: serde_json::Value,
    pub rate_limit: i32,
    pub is_active: bool,
    pub last_used_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateApiKeyRequest {
    pub name: String,
    pub permissions: Option<Vec<String>>,
    pub rate_limit: Option<i32>,
}

#[derive(Debug, Serialize)]
pub struct CreateApiKeyResponse {
    pub name: String,
    pub key: String,
    pub key_prefix: String,
    pub permissions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachePurgeRequest {
    pub domain: Option<String>,
    pub all: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct CachePurgeResponse {
    pub purged: usize,
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct PasswordChangeRequest {
    pub current_password: String,
    pub new_password: String,
}

// ── Admin State ──────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct AdminState {
    pub db: PgPool,
    pub api_keys: ApiKeyStore,
    pub cache: RfoCache,
    pub telemetry: TelemetryTracker,
}

// ── Password Hashing ─────────────────────────────────────────────────────────

fn hash_password(password: &str) -> Result<String, (StatusCode, String)> {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(password.as_bytes());
    hasher.update(b"rfo-admin-salt-v1");
    Ok(hex::encode(hasher.finalize()))
}

fn verify_password(password: &str, hash: &str) -> bool {
    match hash_password(password) {
        Ok(h) => h == hash,
        Err(_) => false,
    }
}

fn generate_token() -> String {
    format!("admin_{}", Uuid::new_v4().to_string().replace('-', ""))
}

// ── Handlers ─────────────────────────────────────────────────────────────────

/// POST /rfo/admin/login
pub async fn admin_login(
    State(state): State<AdminState>,
    Json(req): Json<AdminLoginRequest>,
) -> Result<Json<AdminLoginResponse>, (StatusCode, String)> {
    let row = sqlx::query(
        "SELECT id, username, role, permissions, password_hash FROM admin_users WHERE username = $1",
    )
    .bind(&req.username)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let user = row.ok_or((StatusCode::UNAUTHORIZED, "Invalid credentials".to_string()))?;

    let password_hash: String = user.get("password_hash");
    if !verify_password(&req.password, &password_hash) {
        return Err((StatusCode::UNAUTHORIZED, "Invalid credentials".to_string()));
    }

    let token = generate_token();
    let expires_at = Utc::now() + chrono::Duration::hours(24);

    sqlx::query(
        "INSERT INTO admin_sessions (user_id, token, expires_at) VALUES ($1, $2, $3)",
    )
    .bind(user.get::<Uuid, _>("id"))
    .bind(&token)
    .bind(expires_at)
    .execute(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    sqlx::query("UPDATE admin_users SET last_login = NOW() WHERE id = $1")
        .bind(user.get::<Uuid, _>("id"))
        .execute(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let admin_user = AdminUser {
        id: user.get("id"),
        username: user.get("username"),
        role: user.get("role"),
        permissions: user.get("permissions"),
        last_login: Some(Utc::now()),
        created_at: Utc::now(),
    };

    Ok(Json(AdminLoginResponse {
        token,
        user: admin_user,
        expires_at,
    }))
}

/// POST /rfo/admin/users
pub async fn create_admin_user(
    State(state): State<AdminState>,
    Json(req): Json<CreateAdminRequest>,
) -> Result<(StatusCode, Json<AdminUser>), (StatusCode, String)> {
    let password_hash = hash_password(&req.password)?;
    let role = req.role.unwrap_or_else(|| "admin".to_string());
    let permissions = serde_json::to_value(req.permissions.unwrap_or_else(|| vec!["read".to_string(), "write".to_string()]))
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let row = sqlx::query(
        "INSERT INTO admin_users (username, password_hash, role, permissions) VALUES ($1, $2, $3, $4) RETURNING id, username, role, permissions, created_at",
    )
    .bind(&req.username)
    .bind(&password_hash)
    .bind(&role)
    .bind(&permissions)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        if e.to_string().contains("duplicate key") {
            (StatusCode::CONFLICT, "Username already exists".to_string())
        } else {
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        }
    })?;

    Ok((StatusCode::CREATED, Json(AdminUser {
        id: row.get("id"),
        username: row.get("username"),
        role: row.get("role"),
        permissions: row.get("permissions"),
        last_login: None,
        created_at: row.get("created_at"),
    })))
}

/// GET /rfo/admin/stats
pub async fn get_system_stats(
    State(state): State<AdminState>,
) -> Result<Json<SystemStats>, (StatusCode, String)> {
    let total_sites: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM sites")
        .fetch_one(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let total_handshakes: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM handshake_logs")
        .fetch_one(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let total_audit_events: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM audit_logs")
        .fetch_one(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let active_api_keys: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM api_key_records WHERE is_active = true")
        .fetch_one(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let avg_quality: (Option<f64>,) = sqlx::query_as("SELECT AVG(quality_score)::double precision FROM sites")
        .fetch_one(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let report = state.telemetry.report(state.cache.len());
    let cache_hit_rate = report.cache.hit_ratio * 100.0;

    Ok(Json(SystemStats {
        total_sites: total_sites.0,
        total_handshakes: total_handshakes.0,
        total_audit_events: total_audit_events.0,
        active_api_keys: active_api_keys.0,
        cache_entries: state.cache.len(),
        cache_hit_rate,
        avg_quality_score: avg_quality.0.unwrap_or(0.0),
        uptime_seconds: report.uptime_seconds,
    }))
}

/// GET /rfo/admin/sites
pub async fn list_admin_sites(
    State(state): State<AdminState>,
    Query(q): Query<SiteQuery>,
) -> Result<Json<PaginatedResponse<SiteRecord>>, (StatusCode, String)> {
    let page = q.page.unwrap_or(1).max(1);
    let per_page = q.per_page.unwrap_or(20).min(100);
    let offset = (page - 1) * per_page;
    let sort = q.sort.unwrap_or_else(|| "quality_score".to_string());
    let order = q.order.unwrap_or_else(|| "desc".to_string());
    let order_clause = if order == "asc" { "ASC" } else { "DESC" };

    let sort_column = match sort.as_str() {
        "domain" => "domain_url",
        "created" => "created_at",
        "updated" => "updated_at",
        _ => "quality_score",
    };

    let total: (i64,) = if let Some(ref search) = q.search {
        let pattern = format!("%{}%", search);
        sqlx::query_as("SELECT COUNT(*) FROM sites WHERE domain_url ILIKE $1")
            .bind(&pattern)
            .fetch_one(&state.db)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    } else {
        sqlx::query_as("SELECT COUNT(*) FROM sites")
            .fetch_one(&state.db)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    };

    let data_query = format!(
        "SELECT id, site_id, domain_url, quality_score, coordinates, created_at, updated_at FROM sites {} ORDER BY {} {} LIMIT $1 OFFSET $2",
        if q.search.is_some() { "WHERE domain_url ILIKE $3" } else { "" },
        sort_column,
        order_clause
    );

    let rows = if let Some(ref search) = q.search {
        let pattern = format!("%{}%", search);
        sqlx::query(&data_query)
            .bind(per_page)
            .bind(offset)
            .bind(&pattern)
            .fetch_all(&state.db)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    } else {
        sqlx::query(&data_query)
            .bind(per_page)
            .bind(offset)
            .fetch_all(&state.db)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    };

    let data: Vec<SiteRecord> = rows
        .iter()
        .map(|row| SiteRecord {
            id: row.get("id"),
            site_id: row.get("site_id"),
            domain_url: row.get("domain_url"),
            quality_score: row.get("quality_score"),
            coordinates: row.get("coordinates"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        })
        .collect();

    let total_pages = (total.0 as f64 / per_page as f64).ceil() as i64;

    Ok(Json(PaginatedResponse {
        data,
        total: total.0,
        page,
        per_page,
        total_pages,
    }))
}

/// DELETE /rfo/admin/sites/:domain
pub async fn delete_site(
    State(state): State<AdminState>,
    Path(domain): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let result = sqlx::query("DELETE FROM sites WHERE domain_url = $1")
        .bind(&domain)
        .execute(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if result.rows_affected() == 0 {
        return Err((StatusCode::NOT_FOUND, "Site not found".to_string()));
    }

    state.cache.remove(&domain);

    Ok(Json(serde_json::json!({
        "message": format!("Site '{}' deleted", domain),
        "domain": domain
    })))
}

/// GET /rfo/admin/audit
pub async fn list_audit_logs(
    State(state): State<AdminState>,
    Query(q): Query<AuditQuery>,
) -> Result<Json<PaginatedResponse<AuditLogRecord>>, (StatusCode, String)> {
    let page = q.page.unwrap_or(1).max(1);
    let per_page = q.per_page.unwrap_or(20).min(100);
    let offset = (page - 1) * per_page;

    let mut conditions = Vec::new();
    let mut param_offset = 0;

    if q.event_type.is_some() {
        param_offset += 1;
        conditions.push(format!("event_type = ${}", param_offset));
    }
    if q.severity.is_some() {
        param_offset += 1;
        conditions.push(format!("severity = ${}", param_offset));
    }
    if q.source_ip.is_some() {
        param_offset += 1;
        conditions.push(format!("source_ip = ${}", param_offset));
    }
    if q.from.is_some() {
        param_offset += 1;
        conditions.push(format!("created_at >= ${}", param_offset));
    }
    if q.to.is_some() {
        param_offset += 1;
        conditions.push(format!("created_at <= ${}", param_offset));
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    let count_sql = format!("SELECT COUNT(*) FROM audit_logs {}", where_clause);
    let data_sql = format!(
        "SELECT id, event_type, severity, source_ip, api_key_name, endpoint, method, status_code, message, metadata, created_at FROM audit_logs {} ORDER BY created_at DESC LIMIT ${} OFFSET ${}",
        where_clause,
        param_offset + 1,
        param_offset + 2
    );

    let mut count_query = sqlx::query_scalar::<_, i64>(&count_sql);
    let mut data_query = sqlx::query(&data_sql);

    if let Some(ref et) = q.event_type {
        count_query = count_query.bind(et);
        data_query = data_query.bind(et);
    }
    if let Some(ref sev) = q.severity {
        count_query = count_query.bind(sev);
        data_query = data_query.bind(sev);
    }
    if let Some(ref ip) = q.source_ip {
        count_query = count_query.bind(ip);
        data_query = data_query.bind(ip);
    }
    if let Some(from) = q.from {
        count_query = count_query.bind(from);
        data_query = data_query.bind(from);
    }
    if let Some(to) = q.to {
        count_query = count_query.bind(to);
        data_query = data_query.bind(to);
    }

    let total = count_query
        .fetch_one(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let rows = data_query
        .bind(per_page)
        .bind(offset)
        .fetch_all(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let data: Vec<AuditLogRecord> = rows
        .iter()
        .map(|row| AuditLogRecord {
            id: row.get("id"),
            event_type: row.get("event_type"),
            severity: row.get("severity"),
            source_ip: row.get("source_ip"),
            api_key_name: row.get("api_key_name"),
            endpoint: row.get("endpoint"),
            method: row.get("method"),
            status_code: row.get("status_code"),
            message: row.get("message"),
            metadata: row.get("metadata"),
            created_at: row.get("created_at"),
        })
        .collect();

    let total_pages = (total as f64 / per_page as f64).ceil() as i64;

    Ok(Json(PaginatedResponse {
        data,
        total,
        page,
        per_page,
        total_pages,
    }))
}

/// GET /rfo/admin/keys
pub async fn list_api_keys(
    State(state): State<AdminState>,
) -> Result<Json<Vec<ApiKeyRecord>>, (StatusCode, String)> {
    let rows = sqlx::query(
        "SELECT id, name, key_prefix, permissions, rate_limit, is_active, last_used_at, created_at FROM api_key_records ORDER BY created_at DESC",
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let data: Vec<ApiKeyRecord> = rows
        .iter()
        .map(|row| ApiKeyRecord {
            id: row.get("id"),
            name: row.get("name"),
            key_prefix: row.get("key_prefix"),
            permissions: row.get("permissions"),
            rate_limit: row.get("rate_limit"),
            is_active: row.get("is_active"),
            last_used_at: row.get("last_used_at"),
            created_at: row.get("created_at"),
        })
        .collect();

    Ok(Json(data))
}

/// POST /rfo/admin/keys
pub async fn create_api_key(
    State(state): State<AdminState>,
    Json(req): Json<CreateApiKeyRequest>,
) -> Result<(StatusCode, Json<CreateApiKeyResponse>), (StatusCode, String)> {
    let key = format!("rfo_{}", Uuid::new_v4().to_string().replace('-', ""));
    let key_prefix = key[..8].to_string();
    let permissions = serde_json::to_value(req.permissions.unwrap_or_else(|| vec!["read".to_string()]))
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let rate_limit = req.rate_limit.unwrap_or(100);

    sqlx::query(
        "INSERT INTO api_key_records (name, key_prefix, permissions, rate_limit) VALUES ($1, $2, $3, $4)",
    )
    .bind(&req.name)
    .bind(&key_prefix)
    .bind(&permissions)
    .bind(rate_limit)
    .execute(&state.db)
    .await
    .map_err(|e| {
        if e.to_string().contains("duplicate key") {
            (StatusCode::CONFLICT, "API key name already exists".to_string())
        } else {
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        }
    })?;

    Ok((StatusCode::CREATED, Json(CreateApiKeyResponse {
        name: req.name,
        key,
        key_prefix,
        permissions: permissions.as_array().unwrap_or(&vec![]).iter().filter_map(|v| v.as_str().map(String::from)).collect(),
    })))
}

/// DELETE /rfo/admin/keys/:name
pub async fn revoke_api_key(
    State(state): State<AdminState>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let result = sqlx::query("UPDATE api_key_records SET is_active = false, revoked_at = NOW() WHERE name = $1 AND is_active = true")
        .bind(&name)
        .execute(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if result.rows_affected() == 0 {
        return Err((StatusCode::NOT_FOUND, "Active API key not found".to_string()));
    }

    Ok(Json(serde_json::json!({
        "message": format!("API key '{}' revoked", name),
        "name": name
    })))
}

/// POST /rfo/admin/cache/purge
pub async fn purge_cache(
    State(state): State<AdminState>,
    Json(req): Json<CachePurgeRequest>,
) -> Result<Json<CachePurgeResponse>, (StatusCode, String)> {
    if req.all.unwrap_or(false) {
        let before = state.cache.len();
        state.cache.clear();
        return Ok(Json(CachePurgeResponse {
            purged: before,
            message: format!("Purged {} cache entries", before),
        }));
    }

    if let Some(domain) = &req.domain {
        let existed = state.cache.get(domain).is_some();
        state.cache.remove(domain);
        return Ok(Json(CachePurgeResponse {
            purged: if existed { 1 } else { 0 },
            message: if existed {
                format!("Purged cache for '{}'", domain)
            } else {
                format!("No cache entry for '{}'", domain)
            },
        }));
    }

    Err((StatusCode::BAD_REQUEST, "Provide 'domain' or 'all: true'".to_string()))
}

/// PUT /rfo/admin/users/:id/password
pub async fn change_password(
    State(state): State<AdminState>,
    Path(user_id): Path<Uuid>,
    Json(req): Json<PasswordChangeRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let row = sqlx::query("SELECT password_hash FROM admin_users WHERE id = $1")
        .bind(user_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let user = row.ok_or((StatusCode::NOT_FOUND, "User not found".to_string()))?;

    let current_hash: String = user.get("password_hash");
    if !verify_password(&req.current_password, &current_hash) {
        return Err((StatusCode::UNAUTHORIZED, "Current password is incorrect".to_string()));
    }

    let new_hash = hash_password(&req.new_password)?;

    sqlx::query("UPDATE admin_users SET password_hash = $1, updated_at = NOW() WHERE id = $2")
        .bind(&new_hash)
        .bind(user_id)
        .execute(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(serde_json::json!({
        "message": "Password updated successfully"
    })))
}

/// GET /rfo/admin/health (detailed)
pub async fn admin_health(
    State(state): State<AdminState>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let db_ok = sqlx::query("SELECT 1")
        .execute(&state.db)
        .await
        .is_ok();

    let report = state.telemetry.report(state.cache.len());

    Ok(Json(serde_json::json!({
        "status": "ok",
        "database": if db_ok { "connected" } else { "disconnected" },
        "cache": {
            "entries": state.cache.len(),
            "hits": report.cache.hits,
            "misses": report.cache.misses,
        },
        "protocol_version": "1.0.0",
    })))
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_password_deterministic() {
        let h1 = hash_password("test123").unwrap();
        let h2 = hash_password("test123").unwrap();
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_verify_password_correct() {
        let hash = hash_password("mypass").unwrap();
        assert!(verify_password("mypass", &hash));
    }

    #[test]
    fn test_verify_password_incorrect() {
        let hash = hash_password("mypass").unwrap();
        assert!(!verify_password("wrong", &hash));
    }

    #[test]
    fn test_generate_token_format() {
        let token = generate_token();
        assert!(token.starts_with("admin_"));
        assert!(token.len() > 30);
    }

    #[test]
    fn test_site_query_defaults() {
        let q = SiteQuery {
            page: None,
            per_page: None,
            sort: None,
            order: None,
            search: None,
        };
        assert_eq!(q.page.unwrap_or(1), 1);
        assert_eq!(q.per_page.unwrap_or(20), 20);
    }

    #[test]
    fn test_audit_query_defaults() {
        let q = AuditQuery {
            page: None,
            per_page: None,
            event_type: None,
            severity: None,
            source_ip: None,
            from: None,
            to: None,
        };
        assert_eq!(q.page.unwrap_or(1), 1);
        assert_eq!(q.per_page.unwrap_or(20), 20);
    }

    #[test]
    fn test_cache_purge_request_serialization() {
        let req = CachePurgeRequest {
            domain: Some("example.com".to_string()),
            all: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("example.com"));
    }

    #[test]
    fn test_create_admin_request_defaults() {
        let req = CreateAdminRequest {
            username: "admin".to_string(),
            password: "pass".to_string(),
            role: None,
            permissions: None,
        };
        assert_eq!(req.role, None);
        assert_eq!(req.permissions, None);
    }
}
