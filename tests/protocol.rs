// ─────────────────────────────────────────────────────────────────────────────
// RFO Protocol — Protocol Compliance Tests
// ─────────────────────────────────────────────────────────────────────────────
// Verifying the implementation matches the OpenAPI spec and protocol contract.
// ─────────────────────────────────────────────────────────────────────────────

use std::collections::HashMap;

// ── Protocol Version Compliance ─────────────────────────────────────────────

#[test]
fn test_protocol_version_is_semver() {
    let version = rfo_core::protocol::PROTOCOL_VERSION;
    let parts: Vec<&str> = version.split('.').collect();
    assert_eq!(parts.len(), 3, "Protocol version must be MAJOR.MINOR.PATCH");
    assert!(
        parts[0].parse::<u16>().is_ok(),
        "Major version must be a number"
    );
    assert!(
        parts[1].parse::<u16>().is_ok(),
        "Minor version must be a number"
    );
    assert!(
        parts[2].parse::<u16>().is_ok(),
        "Patch version must be a number"
    );
}

#[test]
fn test_min_supported_version() {
    let current = rfo_core::protocol::ProtocolVersion::current();
    let min = rfo_core::protocol::ProtocolVersion::parse(
        rfo_core::protocol::MIN_SUPPORTED_VERSION,
    )
    .unwrap();
    assert!(
        current >= min,
        "Current version must be >= min supported"
    );
}

#[test]
fn test_version_compatibility_same_major() {
    let v10 = rfo_core::protocol::ProtocolVersion::parse("1.0.0").unwrap();
    let v11 = rfo_core::protocol::ProtocolVersion::parse("1.1.0").unwrap();
    let v19 = rfo_core::protocol::ProtocolVersion::parse("1.9.9").unwrap();
    let min = rfo_core::protocol::ProtocolVersion::parse("1.0.0").unwrap();

    assert!(v10.is_compatible(&min));
    assert!(v11.is_compatible(&min));
    assert!(v19.is_compatible(&min));
}

#[test]
fn test_version_incompatibility_different_major() {
    let v20 = rfo_core::protocol::ProtocolVersion::parse("2.0.0").unwrap();
    let v09 = rfo_core::protocol::ProtocolVersion::parse("0.9.0").unwrap();
    let min = rfo_core::protocol::ProtocolVersion::parse("1.0.0").unwrap();

    assert!(!v20.is_compatible(&min));
    assert!(!v09.is_compatible(&min));
}

// ── Payload Encoding Compliance ─────────────────────────────────────────────

#[test]
fn test_encoding_json_content_type() {
    assert_eq!(
        rfo_core::protocol::PayloadEncoding::Json.content_type(),
        "application/json"
    );
}

#[test]
fn test_encoding_msgpack_content_type() {
    assert_eq!(
        rfo_core::protocol::PayloadEncoding::MessagePack.content_type(),
        "application/msgpack"
    );
}

#[test]
fn test_encoding_from_header_variants() {
    assert!(matches!(
        rfo_core::protocol::PayloadEncoding::from_header("application/json"),
        rfo_core::protocol::PayloadEncoding::Json
    ));
    assert!(matches!(
        rfo_core::protocol::PayloadEncoding::from_header("application/msgpack"),
        rfo_core::protocol::PayloadEncoding::MessagePack
    ));
    assert!(matches!(
        rfo_core::protocol::PayloadEncoding::from_header("msgpack"),
        rfo_core::protocol::PayloadEncoding::MessagePack
    ));
    assert!(matches!(
        rfo_core::protocol::PayloadEncoding::from_header("MESSAGEPACK"),
        rfo_core::protocol::PayloadEncoding::MessagePack
    ));
    assert!(matches!(
        rfo_core::protocol::PayloadEncoding::from_header("text/html"),
        rfo_core::protocol::PayloadEncoding::Json
    ));
}

// ── Capability Negotiation Compliance ───────────────────────────────────────

#[test]
fn test_capability_request_serialization() {
    let req = rfo_core::protocol::CapabilityRequest {
        supported_encodings: vec![
            rfo_core::protocol::PayloadEncoding::Json,
            rfo_core::protocol::PayloadEncoding::MessagePack,
        ],
        supported_features: vec!["handshake".to_string(), "websocket".to_string()],
        protocol_version: "1.0.0".to_string(),
    };

    let json = serde_json::to_string(&req).unwrap();
    assert!(json.contains("application/json"));
    assert!(json.contains("application/msgpack"));
    assert!(json.contains("handshake"));

    let deserialized: rfo_core::protocol::CapabilityRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.supported_encodings.len(), 2);
    assert_eq!(deserialized.supported_features.len(), 2);
}

#[test]
fn test_capability_response_serialization() {
    let resp = rfo_core::protocol::CapabilityResponse {
        negotiated_encoding: rfo_core::protocol::PayloadEncoding::MessagePack,
        supported_features: vec!["handshake".to_string()],
        protocol_version: "1.0.0".to_string(),
        server_capabilities: vec!["handshake".to_string(), "batch-handshake".to_string()],
    };

    let json = serde_json::to_string(&resp).unwrap();
    let deserialized: rfo_core::protocol::CapabilityResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(
        deserialized.negotiated_encoding,
        rfo_core::protocol::PayloadEncoding::MessagePack
    );
}

// ── WebSocket Message Compliance ────────────────────────────────────────────

#[test]
fn test_ws_subscribe_message() {
    let msg = rfo_core::protocol::WsMessage::Subscribe {
        domains: vec!["example.com".to_string()],
    };
    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("subscribe"));
    assert!(json.contains("example.com"));

    let deserialized: rfo_core::protocol::WsMessage = serde_json::from_str(&json).unwrap();
    match deserialized {
        rfo_core::protocol::WsMessage::Subscribe { domains } => {
            assert_eq!(domains.len(), 1);
            assert_eq!(domains[0], "example.com");
        }
        _ => panic!("Expected Subscribe message"),
    }
}

#[test]
fn test_ws_update_message() {
    let msg = rfo_core::protocol::WsMessage::Update {
        domain: "test.com".to_string(),
        quality_score: 85,
        timestamp: "2024-01-01T00:00:00Z".to_string(),
    };
    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("update"));
    assert!(json.contains("test.com"));
    assert!(json.contains("85"));
}

#[test]
fn test_ws_ping_pong() {
    let ping = rfo_core::protocol::WsMessage::Ping;
    let json = serde_json::to_string(&ping).unwrap();
    // Ping/Pong are tagged with {"type":"ping"}
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["type"], "ping");

    let pong = rfo_core::protocol::WsMessage::Pong;
    let json = serde_json::to_string(&pong).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["type"], "pong");
}

#[test]
fn test_ws_error_message() {
    let msg = rfo_core::protocol::WsMessage::Error {
        code: 400,
        message: "Bad request".to_string(),
    };
    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("error"));
    assert!(json.contains("400"));
}

// ── Stream Chunk Compliance ─────────────────────────────────────────────────

#[test]
fn test_stream_chunk_serialization() {
    let chunk = rfo_core::protocol::StreamChunk {
        chunk_index: 0,
        total_chunks: 10,
        data: vec![72, 101, 108, 108, 111], // "Hello"
        checksum: "abc123".to_string(),
    };

    let json = serde_json::to_string(&chunk).unwrap();
    let deserialized: rfo_core::protocol::StreamChunk = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.chunk_index, 0);
    assert_eq!(deserialized.total_chunks, 10);
    assert_eq!(deserialized.data, vec![72, 101, 108, 108, 111]);
}

// ── Handshake Protocol Compliance ───────────────────────────────────────────

#[test]
fn test_handshake_request_required_fields() {
    let req = rfo_core::rfo_protocol::HandshakeRequest {
        domain_url: "https://example.com".to_string(),
        coordinates: HashMap::new(),
        requested_payload: rfo_core::rfo_protocol::PayloadType::Mdoc,
        nonce: "test-nonce-12345678901234".to_string(),
        timestamp: chrono::Utc::now().timestamp(),
    };

    let json = serde_json::to_string(&req).unwrap();
    let deserialized: rfo_core::rfo_protocol::HandshakeRequest =
        serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.domain_url, "https://example.com");
    assert!(matches!(
        deserialized.requested_payload,
        rfo_core::rfo_protocol::PayloadType::Mdoc
    ));
}

#[test]
fn test_handshake_response_structure() {
    let resp = rfo_core::rfo_protocol::HandshakeResponse {
        header: rfo_core::rfo_protocol::RfoHeader::new(
            "site-id-123".to_string(),
            HashMap::new(),
            85,
        ),
        payload: rfo_core::rfo_protocol::Payload::Mdoc(
            rfo_core::rfo_protocol::MiniDocPayload {
                summary: "Test".to_string(),
                token_count: 10,
                qa_pairs: vec![],
            },
        ),
        processing_time_ms: 42,
        nonce: "test-nonce".to_string(),
    };

    let json = serde_json::to_string(&resp).unwrap();
    assert!(json.contains("site-id-123"));
    assert!(json.contains("42"));
}

// ── Quality Score Range Compliance ──────────────────────────────────────────

#[test]
fn test_quality_score_always_in_range() {
    use rfo_core::compiler::calculate_quality_score;
    use rfo_core::rfo_protocol::{FullDocPayload, MiniDocPayload, QaPair};

    // Test with various content configurations
    let test_cases = vec![
        // Empty
        (String::new(), vec![]),
        // Minimal
        ("Short".to_string(), vec![]),
        // Maximal
        ("A".repeat(10000), (0..20).map(|i| QaPair { question: format!("Q{}", i), answer: format!("A{}", i) }).collect()),
    ];

    for (summary, qa_pairs) in test_cases {
        let mdoc = MiniDocPayload {
            summary: summary.clone(),
            token_count: summary.len() / 4,
            qa_pairs,
        };
        let doc = FullDocPayload {
            raw_markdown: "# Title\n\n".repeat(100),
            data_tables: vec!["| A | B |\n|---|---|".to_string()],
            verification_signature: "valid-sig-1234567890123456".to_string(),
        };

        let score = calculate_quality_score(&mdoc, &doc);
        assert!(
            score <= 100,
            "Score {} exceeds maximum with summary len {}",
            score,
            summary.len()
        );
    }
}

// ── RFO Header Compliance ───────────────────────────────────────────────────

#[test]
fn test_rfo_header_quality_clamping() {
    // Quality score must be 0-100
    let header = rfo_core::rfo_protocol::RfoHeader::new(
        "test".to_string(),
        HashMap::new(),
        200, // Way over
    );
    assert_eq!(header.quality_score, 100);

    let header = rfo_core::rfo_protocol::RfoHeader::new(
        "test".to_string(),
        HashMap::new(),
        101, // Just over
    );
    assert_eq!(header.quality_score, 100);
}

// ── MessagePack Encoding Compliance ─────────────────────────────────────────

#[test]
fn test_msgpack_roundtrip_handshake_response() {
    use rfo_core::rfo_protocol::*;

    let resp = HandshakeResponse {
        header: RfoHeader::new(
            "test-site".to_string(),
            HashMap::from([("key".to_string(), "value".to_string())]),
            85,
        ),
        payload: Payload::Mdoc(MiniDocPayload {
            summary: "Test summary".to_string(),
            token_count: 50,
            qa_pairs: vec![QaPair {
                question: "What is RFO?".to_string(),
                answer: "A protocol for AI agents.".to_string(),
            }],
        }),
        processing_time_ms: 42,
        nonce: "test-nonce-123".to_string(),
    };

    // Serialize to MessagePack
    let msgpack = rmp_serde::to_vec(&resp).unwrap();
    assert!(!msgpack.is_empty());

    // Deserialize from MessagePack
    let deserialized: HandshakeResponse = rmp_serde::from_slice(&msgpack).unwrap();
    assert_eq!(deserialized.header.site_id, "test-site");
    assert_eq!(deserialized.processing_time_ms, 42);
    assert_eq!(deserialized.nonce, "test-nonce-123");
}

#[test]
fn test_msgpack_roundtrip_capability_request() {
    let req = rfo_core::protocol::CapabilityRequest {
        supported_encodings: vec![
            rfo_core::protocol::PayloadEncoding::Json,
            rfo_core::protocol::PayloadEncoding::MessagePack,
        ],
        supported_features: vec!["handshake".to_string()],
        protocol_version: "1.0.0".to_string(),
    };

    let msgpack = rmp_serde::to_vec(&req).unwrap();
    let deserialized: rfo_core::protocol::CapabilityRequest = rmp_serde::from_slice(&msgpack).unwrap();
    assert_eq!(deserialized.supported_encodings.len(), 2);
}
