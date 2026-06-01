use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── RfoHeader ──────────────────────────────────────────────────────────────
// Cryptographic node identity + semantic coordinates + live quality index.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RfoHeader {
    pub site_id: String,
    pub coordinates: HashMap<String, String>,
    pub quality_score: u8,
}

impl RfoHeader {
    pub fn new(site_id: String, coordinates: HashMap<String, String>, quality_score: u8) -> Self {
        Self {
            site_id,
            coordinates,
            quality_score: quality_score.min(100),
        }
    }
}

// ── QaPair ─────────────────────────────────────────────────────────────────
// Atomic question-and-answer vector for AEO (Answer Engine Optimization).

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QaPair {
    pub question: String,
    pub answer: String,
}

// ── MiniDocPayload (.mdoc) ─────────────────────────────────────────────────
// Token-optimized summary mapped to LLM context windows (< 1,500 tokens).

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MiniDocPayload {
    pub summary: String,
    pub token_count: usize,
    pub qa_pairs: Vec<QaPair>,
}

// ── FullDocPayload (.doc) ──────────────────────────────────────────────────
// Deep knowledge layout with markdown, data tables, and verification.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FullDocPayload {
    pub raw_markdown: String,
    pub data_tables: Vec<String>,
    pub verification_signature: String,
}

// ── Handshake Protocol ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PayloadType {
    Doc,
    Mdoc,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandshakeRequest {
    pub domain_url: String,
    pub coordinates: HashMap<String, String>,
    pub requested_payload: PayloadType,
    pub nonce: String,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandshakeResponse {
    pub header: RfoHeader,
    pub payload: Payload,
    pub processing_time_ms: u64,
    pub nonce: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Payload {
    Doc(FullDocPayload),
    Mdoc(MiniDocPayload),
}

// ── Cache Entry ────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct CacheEntry {
    pub header: RfoHeader,
    pub doc: FullDocPayload,
    pub mdoc: MiniDocPayload,
    pub cached_at: DateTime<Utc>,
}

// ── Telemetry Log (maps to handshake_logs table) ───────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryLog {
    pub id: Uuid,
    pub site_id: String,
    pub request_timestamp: DateTime<Utc>,
    pub nonce: String,
    pub processing_time_ms: i32,
    pub client_ip: String,
    pub status_code: i32,
    pub created_at: DateTime<Utc>,
}

// ── Site Record (maps to sites table) ──────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SiteRecord {
    pub id: Uuid,
    pub site_id: String,
    pub domain_url: String,
    pub quality_score: i32,
    pub coordinates: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
