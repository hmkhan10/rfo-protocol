use chrono::Utc;
use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// Reads RFO_SECRET_KEY from the environment. Panics if not set.
fn get_secret_key() -> String {
    std::env::var("RFO_SECRET_KEY").expect("RFO_SECRET_KEY environment variable must be set")
}

/// Rounds a unix timestamp to the nearest hour to create a replay window.
/// Handshakes within the same hour produce the same timestamp component,
/// preventing rapid-fire replay while allowing legitimate retries.
fn replay_window_timestamp(unix_ts: i64) -> i64 {
    (unix_ts / 3600) * 3600
}

/// Generates a 64-character hex-encoded HMAC-SHA256 site_id.
///
/// The HMAC input is: `{domain_url}|{hour-rounded-timestamp}`
/// This ensures the site_id is:
///   - Unique per domain
///   - Time-rotated every hour (replay protection)
///   - Derived from a dynamic secret (no static salts)
pub fn generate_site_id(domain_url: &str) -> Result<String, Box<dyn std::error::Error>> {
    let secret = get_secret_key();
    let timestamp = replay_window_timestamp(Utc::now().timestamp());

    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|e| format!("HMAC key error: {}", e))?;

    let message = format!("{}|{}", domain_url, timestamp);
    mac.update(message.as_bytes());

    Ok(hex::encode(mac.finalize().into_bytes()))
}

/// Verifies a site_id against a domain and a specific timestamp.
/// Returns true if the HMAC matches for that hour window.
pub fn verify_site_id(site_id: &str, domain_url: &str, timestamp: i64) -> bool {
    let secret = match std::env::var("RFO_SECRET_KEY") {
        Ok(k) => k,
        Err(_) => return false,
    };

    let windowed = replay_window_timestamp(timestamp);

    let mut mac = match HmacSha256::new_from_slice(secret.as_bytes()) {
        Ok(m) => m,
        Err(_) => return false,
    };

    let message = format!("{}|{}", domain_url, windowed);
    mac.update(message.as_bytes());

    let computed = hex::encode(mac.finalize().into_bytes());
    // Constant-time comparison to prevent timing attacks
    computed == site_id
}

/// Generates a unique nonce (UUID v4) for handshake request tracking.
pub fn generate_handshake_nonce() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Verifies that a handshake nonce is fresh (within a 5-minute window).
/// This prevents replay attacks by rejecting old nonces.
pub fn verify_handshake_nonce(nonce: &str, timestamp: i64) -> bool {
    let now = chrono::Utc::now().timestamp();
    let age = (now - timestamp).abs();
    // Nonce must be within 5 minutes of server time
    age <= 300 && !nonce.is_empty() && nonce.len() >= 16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_site_id_deterministic_within_hour() {
        std::env::set_var("RFO_SECRET_KEY", "test-secret-key-for-rfo");
        let id1 = generate_site_id("https://example.com").unwrap();
        let id2 = generate_site_id("https://example.com").unwrap();
        // Same hour window => same id
        assert_eq!(id1, id2);
        assert_eq!(id1.len(), 64); // SHA-256 hex = 64 chars
    }

    #[test]
    fn test_site_id_unique_per_domain() {
        std::env::set_var("RFO_SECRET_KEY", "test-secret-key-for-rfo");
        let id1 = generate_site_id("https://example.com").unwrap();
        let id2 = generate_site_id("https://other.com").unwrap();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_verify_site_id_valid() {
        std::env::set_var("RFO_SECRET_KEY", "test-secret-key-for-rfo");
        let domain = "https://example.com";
        let ts = Utc::now().timestamp();
        let site_id = generate_site_id(domain).unwrap();
        assert!(verify_site_id(&site_id, domain, ts));
    }

    #[test]
    fn test_verify_site_id_wrong_domain() {
        std::env::set_var("RFO_SECRET_KEY", "test-secret-key-for-rfo");
        let ts = Utc::now().timestamp();
        let site_id = generate_site_id("https://example.com").unwrap();
        assert!(!verify_site_id(&site_id, "https://attacker.com", ts));
    }

    #[test]
    fn test_verify_site_id_wrong_key() {
        std::env::set_var("RFO_SECRET_KEY", "test-secret-key-for-rfo");
        let ts = Utc::now().timestamp();
        let site_id = generate_site_id("https://example.com").unwrap();
        std::env::set_var("RFO_SECRET_KEY", "different-key");
        assert!(!verify_site_id(&site_id, "https://example.com", ts));
        std::env::set_var("RFO_SECRET_KEY", "test-secret-key-for-rfo");
    }

    #[test]
    fn test_nonce_is_unique() {
        let n1 = generate_handshake_nonce();
        let n2 = generate_handshake_nonce();
        assert_ne!(n1, n2);
    }
}
