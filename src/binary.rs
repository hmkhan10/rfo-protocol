use bytes::{BufMut, Bytes, BytesMut};

use crate::rfo_protocol::{FullDocPayload, MiniDocPayload, RfoHeader};

// ── Binary Protocol ──────────────────────────────────────────────────────────
//
// The RFO binary protocol enables efficient native Rust binary transfer
// of .doc and .mdoc payloads. It uses a compact wire format optimized
// for:
//   - Minimal overhead (magic bytes + version + length prefixing)
//   - Zero-copy deserialization where possible
//   - Checksum verification for data integrity
//   - Streaming support for large payloads
//
// Wire format:
//   [magic: 4 bytes] [version: 2 bytes] [type: 1 byte] [length: 4 bytes] [payload: N bytes] [checksum: 4 bytes]
//
// Magic bytes: "RFO\0" (0x52464F00)
// Version: 1.0 (0x0001)
// Type: 0x01 = .mdoc, 0x02 = .doc, 0x03 = batch

/// Magic bytes for the RFO binary protocol.
pub const RFO_MAGIC: [u8; 4] = [0x52, 0x46, 0x4F, 0x00]; // "RFO\0"

/// Current protocol version.
pub const PROTOCOL_VERSION: u16 = 0x0001;

/// Payload type markers.
pub const TYPE_MDOC: u8 = 0x01;
pub const TYPE_DOC: u8 = 0x02;
pub const TYPE_BATCH: u8 = 0x03;

/// Transport-level frame types (0x10+).
pub const TYPE_HANDSHAKE: u8 = 0x10;
pub const TYPE_RESOLVE_OPT: u8 = 0x11;
pub const TYPE_CORE_FILE: u8 = 0x12;
pub const TYPE_ERROR: u8 = 0xFF;

/// Maximum payload size (10MB).
pub const MAX_PAYLOAD_SIZE: usize = 10 * 1024 * 1024;

/// Errors during binary protocol operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BinaryError {
    InvalidMagic,
    UnsupportedVersion(u16),
    InvalidType(u8),
    PayloadTooLarge(usize),
    ChecksumMismatch { expected: u32, actual: u32 },
    SerializationError(String),
    DeserializationError(String),
    IoError(String),
}

impl std::fmt::Display for BinaryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BinaryError::InvalidMagic => write!(f, "invalid magic bytes"),
            BinaryError::UnsupportedVersion(v) => write!(f, "unsupported version: {}", v),
            BinaryError::InvalidType(t) => write!(f, "invalid payload type: {}", t),
            BinaryError::PayloadTooLarge(s) => write!(f, "payload too large: {} bytes", s),
            BinaryError::ChecksumMismatch { expected, actual } => {
                write!(f, "checksum mismatch: expected {:08x}, got {:08x}", expected, actual)
            }
            BinaryError::SerializationError(e) => write!(f, "serialization error: {}", e),
            BinaryError::DeserializationError(e) => write!(f, "deserialization error: {}", e),
            BinaryError::IoError(e) => write!(f, "IO error: {}", e),
        }
    }
}

impl std::error::Error for BinaryError {}

// ── Binary Header ────────────────────────────────────────────────────────────

/// The binary protocol header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct BinaryHeader {
    /// Magic bytes (4 bytes)
    pub magic: [u8; 4],
    /// Protocol version (2 bytes)
    pub version: u16,
    /// Payload type (1 byte)
    pub payload_type: u8,
    /// Payload length (4 bytes)
    pub length: u32,
}

impl BinaryHeader {
    /// Create a new binary header.
    pub fn new(payload_type: u8, length: u32) -> Self {
        BinaryHeader {
            magic: RFO_MAGIC,
            version: PROTOCOL_VERSION,
            payload_type,
            length,
        }
    }

    /// Serialize the header to bytes.
    pub fn to_bytes(&self) -> [u8; 11] {
        let mut buf = [0u8; 11];
        buf[0..4].copy_from_slice(&self.magic);
        buf[4..6].copy_from_slice(&self.version.to_be_bytes());
        buf[6] = self.payload_type;
        buf[7..11].copy_from_slice(&self.length.to_be_bytes());
        buf
    }

    /// Deserialize a header from bytes.
    pub fn from_bytes(bytes: &[u8; 11]) -> Result<Self, BinaryError> {
        let magic = [bytes[0], bytes[1], bytes[2], bytes[3]];
        if magic != RFO_MAGIC {
            return Err(BinaryError::InvalidMagic);
        }

        let version = u16::from_be_bytes([bytes[4], bytes[5]]);
        if version != PROTOCOL_VERSION {
            return Err(BinaryError::UnsupportedVersion(version));
        }

        let payload_type = bytes[6];
        if payload_type != TYPE_MDOC && payload_type != TYPE_DOC && payload_type != TYPE_BATCH
            && payload_type != TYPE_HANDSHAKE && payload_type != TYPE_RESOLVE_OPT
            && payload_type != TYPE_CORE_FILE && payload_type != TYPE_ERROR
        {
            return Err(BinaryError::InvalidType(payload_type));
        }

        let length = u32::from_be_bytes([bytes[7], bytes[8], bytes[9], bytes[10]]);
        if length as usize > MAX_PAYLOAD_SIZE {
            return Err(BinaryError::PayloadTooLarge(length as usize));
        }

        Ok(BinaryHeader {
            magic,
            version,
            payload_type,
            length,
        })
    }

    /// Total header size in bytes.
    pub const fn size() -> usize {
        11
    }
}

// ── Checksum ─────────────────────────────────────────────────────────────────

/// Calculate CRC32 checksum for data integrity.
pub fn calculate_checksum(data: &[u8]) -> u32 {
    // Simple CRC32 implementation
    let mut crc: u32 = 0xFFFFFFFF;
    for byte in data {
        crc ^= *byte as u32;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB88320;
            } else {
                crc >>= 1;
            }
        }
    }
    crc ^ 0xFFFFFFFF
}

// ── Binary Serialization ─────────────────────────────────────────────────────

/// Serialize a .mdoc payload to binary format.
pub fn serialize_mdoc(mdoc: &MiniDocPayload, header: &RfoHeader) -> Result<Bytes, BinaryError> {
    let payload = serde_json::to_vec(&(header, mdoc))
        .map_err(|e| BinaryError::SerializationError(e.to_string()))?;

    let bin_header = BinaryHeader::new(TYPE_MDOC, payload.len() as u32);
    let checksum = calculate_checksum(&payload);

    let mut buf = BytesMut::with_capacity(BinaryHeader::size() + payload.len() + 4);
    buf.put_slice(&bin_header.to_bytes());
    buf.put_slice(&payload);
    buf.put_u32(checksum);

    Ok(buf.freeze())
}

/// Serialize a .doc payload to binary format.
pub fn serialize_doc(doc: &FullDocPayload, header: &RfoHeader) -> Result<Bytes, BinaryError> {
    let payload = serde_json::to_vec(&(header, doc))
        .map_err(|e| BinaryError::SerializationError(e.to_string()))?;

    let bin_header = BinaryHeader::new(TYPE_DOC, payload.len() as u32);
    let checksum = calculate_checksum(&payload);

    let mut buf = BytesMut::with_capacity(BinaryHeader::size() + payload.len() + 4);
    buf.put_slice(&bin_header.to_bytes());
    buf.put_slice(&payload);
    buf.put_u32(checksum);

    Ok(buf.freeze())
}

/// Serialize a batch of payloads to binary format.
pub fn serialize_batch(
    items: &[(RfoHeader, MiniDocPayload, FullDocPayload)],
) -> Result<Bytes, BinaryError> {
    let payload = serde_json::to_vec(items)
        .map_err(|e| BinaryError::SerializationError(e.to_string()))?;

    let bin_header = BinaryHeader::new(TYPE_BATCH, payload.len() as u32);
    let checksum = calculate_checksum(&payload);

    let mut buf = BytesMut::with_capacity(BinaryHeader::size() + payload.len() + 4);
    buf.put_slice(&bin_header.to_bytes());
    buf.put_slice(&payload);
    buf.put_u32(checksum);

    Ok(buf.freeze())
}

// ── Binary Deserialization ───────────────────────────────────────────────────

/// Deserialize a .mdoc payload from binary format.
pub fn deserialize_mdoc(data: &[u8]) -> Result<(RfoHeader, MiniDocPayload), BinaryError> {
    if data.len() < BinaryHeader::size() + 4 {
        return Err(BinaryError::DeserializationError("data too short".to_string()));
    }

    let header_bytes: [u8; 11] = data[..11]
        .try_into()
        .map_err(|_| BinaryError::DeserializationError("invalid header".to_string()))?;
    let bin_header = BinaryHeader::from_bytes(&header_bytes)?;

    if bin_header.payload_type != TYPE_MDOC {
        return Err(BinaryError::InvalidType(bin_header.payload_type));
    }

    let payload_start = BinaryHeader::size();
    let payload_end = payload_start + bin_header.length as usize;
    let checksum_start = payload_end;

    let payload = &data[payload_start..payload_end];
    let expected_checksum = u32::from_be_bytes([
        data[checksum_start],
        data[checksum_start + 1],
        data[checksum_start + 2],
        data[checksum_start + 3],
    ]);
    let actual_checksum = calculate_checksum(payload);

    if expected_checksum != actual_checksum {
        return Err(BinaryError::ChecksumMismatch {
            expected: expected_checksum,
            actual: actual_checksum,
        });
    }

    serde_json::from_slice(payload)
        .map_err(|e| BinaryError::DeserializationError(e.to_string()))
}

/// Deserialize a .doc payload from binary format.
pub fn deserialize_doc(data: &[u8]) -> Result<(RfoHeader, FullDocPayload), BinaryError> {
    if data.len() < BinaryHeader::size() + 4 {
        return Err(BinaryError::DeserializationError("data too short".to_string()));
    }

    let header_bytes: [u8; 11] = data[..11]
        .try_into()
        .map_err(|_| BinaryError::DeserializationError("invalid header".to_string()))?;
    let bin_header = BinaryHeader::from_bytes(&header_bytes)?;

    if bin_header.payload_type != TYPE_DOC {
        return Err(BinaryError::InvalidType(bin_header.payload_type));
    }

    let payload_start = BinaryHeader::size();
    let payload_end = payload_start + bin_header.length as usize;
    let checksum_start = payload_end;

    let payload = &data[payload_start..payload_end];
    let expected_checksum = u32::from_be_bytes([
        data[checksum_start],
        data[checksum_start + 1],
        data[checksum_start + 2],
        data[checksum_start + 3],
    ]);
    let actual_checksum = calculate_checksum(payload);

    if expected_checksum != actual_checksum {
        return Err(BinaryError::ChecksumMismatch {
            expected: expected_checksum,
            actual: actual_checksum,
        });
    }

    serde_json::from_slice(payload)
        .map_err(|e| BinaryError::DeserializationError(e.to_string()))
}

/// Deserialize a batch from binary format.
pub fn deserialize_batch(
    data: &[u8],
) -> Result<Vec<(RfoHeader, MiniDocPayload, FullDocPayload)>, BinaryError> {
    if data.len() < BinaryHeader::size() + 4 {
        return Err(BinaryError::DeserializationError("data too short".to_string()));
    }

    let header_bytes: [u8; 11] = data[..11]
        .try_into()
        .map_err(|_| BinaryError::DeserializationError("invalid header".to_string()))?;
    let bin_header = BinaryHeader::from_bytes(&header_bytes)?;

    if bin_header.payload_type != TYPE_BATCH {
        return Err(BinaryError::InvalidType(bin_header.payload_type));
    }

    let payload_start = BinaryHeader::size();
    let payload_end = payload_start + bin_header.length as usize;
    let checksum_start = payload_end;

    let payload = &data[payload_start..payload_end];
    let expected_checksum = u32::from_be_bytes([
        data[checksum_start],
        data[checksum_start + 1],
        data[checksum_start + 2],
        data[checksum_start + 3],
    ]);
    let actual_checksum = calculate_checksum(payload);

    if expected_checksum != actual_checksum {
        return Err(BinaryError::ChecksumMismatch {
            expected: expected_checksum,
            actual: actual_checksum,
        });
    }

    serde_json::from_slice(payload)
        .map_err(|e| BinaryError::DeserializationError(e.to_string()))
}

// ── Streaming ────────────────────────────────────────────────────────────────

/// A streaming binary reader for large payloads.
pub struct BinaryStreamReader {
    header: BinaryHeader,
    data: Bytes,
    position: usize,
    checksum_validated: bool,
}

impl BinaryStreamReader {
    /// Create a new streaming reader from raw bytes.
    pub fn new(data: Bytes) -> Result<Self, BinaryError> {
        if data.len() < BinaryHeader::size() + 4 {
            return Err(BinaryError::DeserializationError("data too short".to_string()));
        }

        let header_bytes: [u8; 11] = data[..11]
            .try_into()
            .map_err(|_| BinaryError::DeserializationError("invalid header".to_string()))?;
        let header = BinaryHeader::from_bytes(&header_bytes)?;

        Ok(BinaryStreamReader {
            header,
            data,
            position: BinaryHeader::size(),
            checksum_validated: false,
        })
    }

    /// Read a chunk of the payload.
    pub fn read_chunk(&mut self, max_bytes: usize) -> Result<Vec<u8>, BinaryError> {
        let payload_end = BinaryHeader::size() + self.header.length as usize;
        let remaining = payload_end.saturating_sub(self.position);
        let to_read = max_bytes.min(remaining);

        if to_read == 0 {
            return Ok(vec![]);
        }

        let chunk = self.data[self.position..self.position + to_read].to_vec();
        self.position += to_read;

        Ok(chunk)
    }

    /// Validate the checksum after reading all data.
    pub fn validate_checksum(&mut self) -> Result<bool, BinaryError> {
        if self.checksum_validated {
            return Ok(true);
        }

        let payload_start = BinaryHeader::size();
        let payload_end = payload_start + self.header.length as usize;
        let checksum_start = payload_end;

        let payload = &self.data[payload_start..payload_end];
        let expected_checksum = u32::from_be_bytes([
            self.data[checksum_start],
            self.data[checksum_start + 1],
            self.data[checksum_start + 2],
            self.data[checksum_start + 3],
        ]);
        let actual_checksum = calculate_checksum(payload);

        self.checksum_validated = true;
        Ok(expected_checksum == actual_checksum)
    }

    /// Get the payload type.
    pub fn payload_type(&self) -> u8 {
        self.header.payload_type
    }

    /// Get the payload length.
    pub fn payload_length(&self) -> u32 {
        self.header.length
    }

    /// Check if all data has been read.
    pub fn is_complete(&self) -> bool {
        self.position >= BinaryHeader::size() + self.header.length as usize
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rfo_protocol::QaPair;
    use std::collections::HashMap;

    fn test_header() -> RfoHeader {
        RfoHeader::new("test-site-id".to_string(), HashMap::new(), 85)
    }

    fn test_mdoc() -> MiniDocPayload {
        MiniDocPayload {
            summary: "Test summary for binary protocol".to_string(),
            token_count: 100,
            qa_pairs: vec![
                QaPair {
                    question: "What is RFO?".to_string(),
                    answer: "A protocol for AI agents.".to_string(),
                },
            ],
        }
    }

    fn test_doc() -> FullDocPayload {
        FullDocPayload {
            raw_markdown: "# Test\n\nThis is a test document for binary protocol testing.".to_string(),
            data_tables: vec![],
            verification_signature: "test-signature".to_string(),
        }
    }

    #[test]
    fn test_header_serialization() {
        let header = BinaryHeader::new(TYPE_MDOC, 1024);
        let bytes = header.to_bytes();
        assert_eq!(bytes.len(), 11);
        assert_eq!(&bytes[0..4], &RFO_MAGIC);
        assert_eq!(bytes[6], TYPE_MDOC);
    }

    #[test]
    fn test_header_deserialization() {
        let header = BinaryHeader::new(TYPE_DOC, 2048);
        let bytes = header.to_bytes();
        let restored = BinaryHeader::from_bytes(&bytes).unwrap();
        assert_eq!(restored.magic, RFO_MAGIC);
        assert_eq!(restored.version, PROTOCOL_VERSION);
        assert_eq!(restored.payload_type, TYPE_DOC);
        assert_eq!(restored.length, 2048);
    }

    #[test]
    fn test_invalid_magic() {
        let mut bytes = [0u8; 11];
        bytes[0..4].copy_from_slice(b"BADD");
        assert_eq!(BinaryHeader::from_bytes(&bytes), Err(BinaryError::InvalidMagic));
    }

    #[test]
    fn test_checksum() {
        let data = b"hello world";
        let checksum = calculate_checksum(data);
        assert!(checksum != 0);
        assert_eq!(checksum, calculate_checksum(data));
    }

    #[test]
    fn test_serialize_mdoc_roundtrip() {
        let header = test_header();
        let mdoc = test_mdoc();

        let binary = serialize_mdoc(&mdoc, &header).unwrap();
        let (restored_header, restored_mdoc) = deserialize_mdoc(&binary).unwrap();

        assert_eq!(restored_header.site_id, header.site_id);
        assert_eq!(restored_mdoc.summary, mdoc.summary);
        assert_eq!(restored_mdoc.token_count, mdoc.token_count);
        assert_eq!(restored_mdoc.qa_pairs.len(), mdoc.qa_pairs.len());
    }

    #[test]
    fn test_serialize_doc_roundtrip() {
        let header = test_header();
        let doc = test_doc();

        let binary = serialize_doc(&doc, &header).unwrap();
        let (restored_header, restored_doc) = deserialize_doc(&binary).unwrap();

        assert_eq!(restored_header.site_id, header.site_id);
        assert_eq!(restored_doc.raw_markdown, doc.raw_markdown);
        assert_eq!(restored_doc.verification_signature, doc.verification_signature);
    }

    #[test]
    fn test_serialize_batch_roundtrip() {
        let header = test_header();
        let mdoc = test_mdoc();
        let doc = test_doc();

        let binary = serialize_batch(&[(header.clone(), mdoc.clone(), doc.clone())]).unwrap();
        let restored = deserialize_batch(&binary).unwrap();

        assert_eq!(restored.len(), 1);
        assert_eq!(restored[0].0.site_id, header.site_id);
        assert_eq!(restored[0].1.summary, mdoc.summary);
        assert_eq!(restored[0].2.raw_markdown, doc.raw_markdown);
    }

    #[test]
    fn test_streaming_reader() {
        let header = test_header();
        let mdoc = test_mdoc();

        let binary = serialize_mdoc(&mdoc, &header).unwrap();
        let mut reader = BinaryStreamReader::new(binary).unwrap();

        assert_eq!(reader.payload_type(), TYPE_MDOC);
        assert!(!reader.is_complete());

        let mut all_data = Vec::new();
        while let Ok(chunk) = reader.read_chunk(10) {
            if chunk.is_empty() {
                break;
            }
            all_data.extend(chunk);
        }

        assert!(reader.is_complete());
        assert!(reader.validate_checksum().unwrap());
    }

    #[test]
    fn test_payload_too_large() {
        let header = BinaryHeader::new(TYPE_MDOC, MAX_PAYLOAD_SIZE as u32 + 1);
        assert_eq!(
            BinaryHeader::from_bytes(&header.to_bytes()),
            Err(BinaryError::PayloadTooLarge(MAX_PAYLOAD_SIZE + 1))
        );
    }

    #[test]
    fn test_checksum_mismatch() {
        let header = test_header();
        let mdoc = test_mdoc();

        let mut binary = serialize_mdoc(&mdoc, &header).unwrap().to_vec();
        // Corrupt the checksum
        let last = binary.len() - 1;
        binary[last] ^= 0xFF;

        assert!(deserialize_mdoc(&binary).is_err());
    }
}
