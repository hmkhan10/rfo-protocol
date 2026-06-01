use serde::{Deserialize, Serialize};

// ── Protocol Version ───────────────────────────────────────────────────────

pub const PROTOCOL_VERSION: &str = "1.0.0";
pub const MIN_SUPPORTED_VERSION: &str = "1.0.0";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProtocolVersion {
    pub major: u16,
    pub minor: u16,
    pub patch: u16,
}

impl ProtocolVersion {
    pub fn current() -> Self {
        Self::parse(PROTOCOL_VERSION).unwrap()
    }

    pub fn parse(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() != 3 {
            return None;
        }
        Some(Self {
            major: parts[0].parse().ok()?,
            minor: parts[1].parse().ok()?,
            patch: parts[2].parse().ok()?,
        })
    }

    pub fn is_compatible(&self, min: &ProtocolVersion) -> bool {
        self.major == min.major && self >= min
    }
}

impl std::fmt::Display for ProtocolVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

// ── Binary Payload Support (MessagePack) ───────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PayloadEncoding {
    #[serde(rename = "application/json")]
    Json,
    #[serde(rename = "application/msgpack")]
    MessagePack,
}

impl PayloadEncoding {
    pub fn from_header(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "msgpack" | "messagepack" | "application/msgpack" => Self::MessagePack,
            _ => Self::Json,
        }
    }

    pub fn content_type(&self) -> &'static str {
        match self {
            Self::Json => "application/json",
            Self::MessagePack => "application/msgpack",
        }
    }
}

// ── Streaming Types ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamChunk {
    pub chunk_index: u32,
    pub total_chunks: u32,
    pub data: Vec<u8>,
    pub checksum: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamRequest {
    pub domain_url: String,
    pub payload_type: crate::rfo_protocol::PayloadType,
    pub chunk_size: Option<usize>,
}

// ── WebSocket Types ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum WsMessage {
    #[serde(rename = "subscribe")]
    Subscribe { domains: Vec<String> },
    #[serde(rename = "unsubscribe")]
    Unsubscribe { domains: Vec<String> },
    #[serde(rename = "handshake")]
    Handshake(crate::rfo_protocol::HandshakeRequest),
    #[serde(rename = "update")]
    Update { domain: String, quality_score: u8, timestamp: String },
    #[serde(rename = "ping")]
    Ping,
    #[serde(rename = "pong")]
    Pong,
    #[serde(rename = "error")]
    Error { code: u16, message: String },
}

// ── Capability Negotiation ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityRequest {
    pub supported_encodings: Vec<PayloadEncoding>,
    pub supported_features: Vec<String>,
    pub protocol_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityResponse {
    pub negotiated_encoding: PayloadEncoding,
    pub supported_features: Vec<String>,
    pub protocol_version: String,
    pub server_capabilities: Vec<String>,
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protocol_version_parse() {
        let v = ProtocolVersion::parse("1.2.3").unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 3);
    }

    #[test]
    fn test_protocol_version_display() {
        let v = ProtocolVersion::parse("1.0.0").unwrap();
        assert_eq!(v.to_string(), "1.0.0");
    }

    #[test]
    fn test_protocol_version_compatible() {
        let v1 = ProtocolVersion::parse("1.0.0").unwrap();
        let v2 = ProtocolVersion::parse("1.1.0").unwrap();
        let min = ProtocolVersion::parse("1.0.0").unwrap();

        assert!(v1.is_compatible(&min));
        assert!(v2.is_compatible(&min));
    }

    #[test]
    fn test_protocol_version_incompatible() {
        let v1 = ProtocolVersion::parse("2.0.0").unwrap();
        let min = ProtocolVersion::parse("1.0.0").unwrap();

        // Major version mismatch = incompatible
        assert!(!v1.is_compatible(&min));
    }

    #[test]
    fn test_payload_encoding_from_header() {
        assert!(matches!(
            PayloadEncoding::from_header("application/json"),
            PayloadEncoding::Json
        ));
        assert!(matches!(
            PayloadEncoding::from_header("application/msgpack"),
            PayloadEncoding::MessagePack
        ));
        assert!(matches!(
            PayloadEncoding::from_header("msgpack"),
            PayloadEncoding::MessagePack
        ));
    }

    #[test]
    fn test_payload_encoding_content_type() {
        assert_eq!(PayloadEncoding::Json.content_type(), "application/json");
        assert_eq!(
            PayloadEncoding::MessagePack.content_type(),
            "application/msgpack"
        );
    }

    #[test]
    fn test_current_version() {
        let v = ProtocolVersion::current();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 0);
        assert_eq!(v.patch, 0);
    }
}
