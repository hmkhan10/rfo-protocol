pub mod site_id;

use sha2::{Digest, Sha256, Sha512};
use hmac::{Hmac, Mac};

type HmacSha256 = Hmac<Sha256>;
type HmacSha512 = Hmac<Sha512>;

// ── Production Cryptography Module ───────────────────────────────────────────
//
// Advanced cryptographic operations for the RFO protocol:
//   - HMAC-SHA256/SHA512: Request signing and verification
//   - Key Derivation: HKDF for key expansion
//   - Nonce Generation: Cryptographically secure random nonces
//   - Content Integrity: SHA-256 hashing for payload verification
//   - Domain Binding: Cryptographic site identity

pub struct RfoCrypto;

impl RfoCrypto {
    /// Compute HMAC-SHA256 of data.
    pub fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
        let mut mac = HmacSha256::new_from_slice(key)
            .expect("HMAC can take key of any size");
        mac.update(data);
        mac.finalize().into_bytes().into()
    }

    /// Compute HMAC-SHA512 of data.
    pub fn hmac_sha512(key: &[u8], data: &[u8]) -> [u8; 64] {
        let mut mac = HmacSha512::new_from_slice(key)
            .expect("HMAC can take key of any size");
        mac.update(data);
        mac.finalize().into_bytes().into()
    }

    /// Verify HMAC-SHA256 signature (constant-time comparison).
    pub fn verify_hmac_sha256(key: &[u8], data: &[u8], expected: &[u8; 32]) -> bool {
        let mut mac = HmacSha256::new_from_slice(key)
            .expect("HMAC can take key of any size");
        mac.update(data);
        mac.verify_slice(expected).is_ok()
    }

    /// Verify HMAC-SHA512 signature (constant-time comparison).
    pub fn verify_hmac_sha512(key: &[u8], data: &[u8], expected: &[u8; 64]) -> bool {
        let mut mac = HmacSha512::new_from_slice(key)
            .expect("HMAC can take key of any size");
        mac.update(data);
        mac.verify_slice(expected).is_ok()
    }

    /// Compute SHA-256 hash of data.
    pub fn sha256(data: &[u8]) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(data);
        hasher.finalize().into()
    }

    /// Compute SHA-512 hash of data.
    pub fn sha512(data: &[u8]) -> [u8; 64] {
        let mut hasher = Sha512::new();
        hasher.update(data);
        hasher.finalize().into()
    }

    /// Compute SHA-256 hash of a string (hex-encoded).
    pub fn hash_content(content: &str) -> String {
        hex::encode(Self::sha256(content.as_bytes()))
    }

    /// Derive a key using HKDF-SHA256.
    pub fn derive_key(secret: &[u8], salt: &[u8], info: &[u8], length: usize) -> Vec<u8> {
        let mut mac = HmacSha256::new_from_slice(salt)
            .expect("HMAC can take key of any size");
        mac.update(secret);
        let prk = mac.finalize().into_bytes();

        let mut output = Vec::with_capacity(length);
        let mut block = [0u8; 32];
        let mut counter = 1u8;

        while output.len() < length {
            let mut mac = HmacSha256::new_from_slice(&prk)
                .expect("HMAC can take key of any size");
            if counter > 1 {
                mac.update(&block);
            }
            mac.update(info);
            mac.update(&[counter]);
            block = mac.finalize().into_bytes().into();

            let remaining = length - output.len();
            let to_take = remaining.min(32);
            output.extend_from_slice(&block[..to_take]);
            counter += 1;
        }

        output
    }

    /// Generate a cryptographically secure random nonce.
    pub fn generate_nonce() -> String {
        use uuid::Uuid;
        Uuid::new_v4().to_string()
    }

    /// Verify nonce format (UUID v4).
    pub fn verify_nonce_format(nonce: &str) -> bool {
        uuid::Uuid::parse_str(nonce).is_ok()
    }

    /// Verify nonce freshness (within window).
    pub fn verify_nonce_freshness(timestamp: i64, window_secs: i64) -> bool {
        use chrono::Utc;
        let now = Utc::now().timestamp();
        (now - timestamp).abs() <= window_secs
    }

    /// Generate a content integrity hash.
    pub fn content_hash(domain: &str, content: &str, timestamp: i64) -> String {
        let payload = format!("{}|{}|{}", domain, content, timestamp);
        hex::encode(Self::sha256(payload.as_bytes()))
    }

    /// Verify content integrity.
    pub fn verify_content_integrity(
        domain: &str,
        content: &str,
        timestamp: i64,
        expected_hash: &str,
    ) -> bool {
        let actual_hash = Self::content_hash(domain, content, timestamp);
        actual_hash == expected_hash
    }

    /// Generate a domain-bound site ID.
    pub fn bind_domain(secret: &str, domain: &str, hour_window: i64) -> String {
        let payload = format!("{}|{}|{}", secret, domain, hour_window);
        hex::encode(Self::sha256(payload.as_bytes()))
    }

    /// Verify domain binding.
    pub fn verify_domain_binding(
        secret: &str,
        domain: &str,
        hour_window: i64,
        expected_site_id: &str,
    ) -> bool {
        let actual_site_id = Self::bind_domain(secret, domain, hour_window);
        actual_site_id == expected_site_id
    }

    /// Sign a request body with HMAC-SHA256.
    pub fn sign_request(secret: &str, method: &str, path: &str, body: &[u8], timestamp: i64) -> String {
        let payload = format!("{}|{}|{}|{}", method, path, timestamp, hex::encode(Self::sha256(body)));
        let signature = Self::hmac_sha256(secret.as_bytes(), payload.as_bytes());
        hex::encode(signature)
    }

    /// Verify a request signature.
    pub fn verify_request_signature(
        secret: &str,
        method: &str,
        path: &str,
        body: &[u8],
        timestamp: i64,
        expected_signature: &str,
    ) -> bool {
        let actual_signature = Self::sign_request(secret, method, path, body, timestamp);
        actual_signature == expected_signature
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_KEY: &[u8] = b"test-secret-key-for-crypto-operations";
    const TEST_SECRET: &str = "test-secret-key-for-crypto-operations";

    #[test]
    fn test_hmac_sha256_deterministic() {
        let h1 = RfoCrypto::hmac_sha256(TEST_KEY, b"hello");
        let h2 = RfoCrypto::hmac_sha256(TEST_KEY, b"hello");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_hmac_sha256_different_data() {
        let h1 = RfoCrypto::hmac_sha256(TEST_KEY, b"hello");
        let h2 = RfoCrypto::hmac_sha256(TEST_KEY, b"world");
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_verify_hmac_sha256_valid() {
        let data = b"test data";
        let signature = RfoCrypto::hmac_sha256(TEST_KEY, data);
        assert!(RfoCrypto::verify_hmac_sha256(TEST_KEY, data, &signature));
    }

    #[test]
    fn test_verify_hmac_sha256_invalid() {
        let data = b"test data";
        let wrong_data = b"wrong data";
        let signature = RfoCrypto::hmac_sha256(TEST_KEY, data);
        assert!(!RfoCrypto::verify_hmac_sha256(TEST_KEY, wrong_data, &signature));
    }

    #[test]
    fn test_sha256_deterministic() {
        let h1 = RfoCrypto::sha256(b"hello");
        let h2 = RfoCrypto::sha256(b"hello");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_hash_content() {
        let hash = RfoCrypto::hash_content("test content");
        assert_eq!(hash.len(), 64);
    }

    #[test]
    fn test_derive_key() {
        let key = RfoCrypto::derive_key(b"secret", b"salt", b"info", 32);
        assert_eq!(key.len(), 32);
    }

    #[test]
    fn test_generate_nonce() {
        let nonce = RfoCrypto::generate_nonce();
        assert!(!nonce.is_empty());
        assert!(RfoCrypto::verify_nonce_format(&nonce));
    }

    #[test]
    fn test_verify_nonce_format() {
        assert!(RfoCrypto::verify_nonce_format("550e8400-e29b-41d4-a716-446655440000"));
        assert!(!RfoCrypto::verify_nonce_format("invalid-nonce"));
    }

    #[test]
    fn test_verify_nonce_freshness() {
        use chrono::Utc;
        let now = Utc::now().timestamp();
        assert!(RfoCrypto::verify_nonce_freshness(now, 300));
        assert!(!RfoCrypto::verify_nonce_freshness(now - 600, 300));
    }

    #[test]
    fn test_content_hash() {
        let hash1 = RfoCrypto::content_hash("example.com", "content", 1234567890);
        let hash2 = RfoCrypto::content_hash("example.com", "content", 1234567890);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_verify_content_integrity() {
        let hash = RfoCrypto::content_hash("example.com", "content", 1234567890);
        assert!(RfoCrypto::verify_content_integrity("example.com", "content", 1234567890, &hash));
        assert!(!RfoCrypto::verify_content_integrity("example.com", "tampered", 1234567890, &hash));
    }

    #[test]
    fn test_domain_binding() {
        let site_id = RfoCrypto::bind_domain(TEST_SECRET, "example.com", 100);
        assert!(RfoCrypto::verify_domain_binding(TEST_SECRET, "example.com", 100, &site_id));
        assert!(!RfoCrypto::verify_domain_binding(TEST_SECRET, "example.com", 101, &site_id));
    }

    #[test]
    fn test_sign_request() {
        let signature = RfoCrypto::sign_request(TEST_SECRET, "POST", "/rfo/handshake", b"body", 1234567890);
        assert!(RfoCrypto::verify_request_signature(TEST_SECRET, "POST", "/rfo/handshake", b"body", 1234567890, &signature));
    }

    #[test]
    fn test_sign_request_tampered() {
        let signature = RfoCrypto::sign_request(TEST_SECRET, "POST", "/rfo/handshake", b"body", 1234567890);
        assert!(!RfoCrypto::verify_request_signature(TEST_SECRET, "POST", "/rfo/handshake", b"tampered", 1234567890, &signature));
    }
}
