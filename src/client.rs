use std::collections::HashMap;
use std::time::Duration;

use reqwest::Client;

use crate::rfo_protocol::{
    FullDocPayload, HandshakeRequest, HandshakeResponse, MiniDocPayload, PayloadType,
};

// ── Errors ─────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum RfoError {
    Network(String),
    Server { status: u16, body: String },
    Serialization(String),
    Timeout,
    RetryExhausted { attempts: u32, last_error: String },
}

impl std::fmt::Display for RfoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RfoError::Network(e) => write!(f, "network error: {}", e),
            RfoError::Server { status, body } => write!(f, "server error {}: {}", status, body),
            RfoError::Serialization(e) => write!(f, "serialization error: {}", e),
            RfoError::Timeout => write!(f, "request timed out"),
            RfoError::RetryExhausted { attempts, last_error } => {
                write!(
                    f,
                    "retry exhausted after {} attempts, last error: {}",
                    attempts, last_error
                )
            }
        }
    }
}

impl std::error::Error for RfoError {}

impl From<reqwest::Error> for RfoError {
    fn from(e: reqwest::Error) -> Self {
        if e.is_timeout() {
            RfoError::Timeout
        } else {
            RfoError::Network(e.to_string())
        }
    }
}

impl From<serde_json::Error> for RfoError {
    fn from(e: serde_json::Error) -> Self {
        RfoError::Serialization(e.to_string())
    }
}

// ── Retry Configuration ────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub max_attempts: u32,
    pub initial_backoff: Duration,
    pub max_backoff: Duration,
    pub backoff_multiplier: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_backoff: Duration::from_millis(200),
            max_backoff: Duration::from_secs(5),
            backoff_multiplier: 2.0,
        }
    }
}

// ── RfoClient ──────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct RfoClient {
    base_url: String,
    http: Client,
    retry: RetryConfig,
}

impl RfoClient {
    /// Create a new client pointing at an RFO engine node.
    ///
    /// # Example
    /// ```no_run
    /// use rfo_core::client::RfoClient;
    /// let client = RfoClient::new("http://localhost:3000");
    /// ```
    pub fn new(base_url: &str) -> Self {
        let http = Client::builder()
            .timeout(Duration::from_secs(10))
            .connect_timeout(Duration::from_secs(3))
            .pool_max_idle_per_host(10)
            .build()
            .expect("Failed to build HTTP client");

        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            http,
            retry: RetryConfig::default(),
        }
    }

    /// Create a client with custom retry configuration.
    pub fn with_retry(base_url: &str, retry: RetryConfig) -> Self {
        let mut client = Self::new(base_url);
        client.retry = retry;
        client
    }

    /// Create a client with a custom HTTP client (for TLS config, proxies, etc.).
    pub fn with_http_client(base_url: &str, http: Client, retry: RetryConfig) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            http,
            retry,
        }
    }

    // ── Core Methods ────────────────────────────────────────────────────

    /// Perform a duplex handshake with an RFO node.
    ///
    /// Automatically generates nonce + timestamp. Returns the full response
    /// including header, payload, and processing telemetry.
    pub async fn handshake(
        &self,
        domain_url: &str,
        coordinates: HashMap<String, String>,
        payload_type: PayloadType,
    ) -> Result<HandshakeResponse, RfoError> {
        let request = HandshakeRequest {
            domain_url: domain_url.to_string(),
            coordinates,
            requested_payload: payload_type,
            nonce: uuid::Uuid::new_v4().to_string(),
            timestamp: chrono::Utc::now().timestamp(),
        };

        let url = format!("{}/rfo/handshake", self.base_url);
        let body = serde_json::to_string(&request)?;

        let response = self
            .execute_with_retry(|client| {
                let url = url.clone();
                let body = body.clone();
                let client = client.clone();
                async move {
                    client
                        .post(&url)
                        .header("Content-Type", "application/json")
                        .body(body.clone())
                        .send()
                        .await
                }
            })
            .await?;

        let status = response.status();
        if !status.is_success() {
            let body_text = response.text().await.unwrap_or_default();
            return Err(RfoError::Server {
                status: status.as_u16(),
                body: body_text,
            });
        }

        let handshake: HandshakeResponse = response.json().await?;
        Ok(handshake)
    }

    /// Fetch a compiled .doc (FullDocPayload) for a domain.
    pub async fn get_doc(&self, domain: &str) -> Result<FullDocPayload, RfoError> {
        let url = format!("{}/rfo/doc/{}", self.base_url, domain);

        let response = self
            .execute_with_retry(|client| {
                let url = url.clone();
                let client = client.clone();
                async move { client.get(&url).send().await }
            })
            .await?;

        let status = response.status();
        if !status.is_success() {
            let body_text = response.text().await.unwrap_or_default();
            return Err(RfoError::Server {
                status: status.as_u16(),
                body: body_text,
            });
        }

        let doc: FullDocPayload = response.json().await?;
        Ok(doc)
    }

    /// Fetch a compiled .mdoc (MiniDocPayload) for a domain.
    pub async fn get_mdoc(&self, domain: &str) -> Result<MiniDocPayload, RfoError> {
        let url = format!("{}/rfo/mdoc/{}", self.base_url, domain);

        let response = self
            .execute_with_retry(|client| {
                let url = url.clone();
                let client = client.clone();
                async move { client.get(&url).send().await }
            })
            .await?;

        let status = response.status();
        if !status.is_success() {
            let body_text = response.text().await.unwrap_or_default();
            return Err(RfoError::Server {
                status: status.as_u16(),
                body: body_text,
            });
        }

        let mdoc: MiniDocPayload = response.json().await?;
        Ok(mdoc)
    }

    /// List all registered sites from the engine.
    pub async fn list_sites(&self) -> Result<Vec<serde_json::Value>, RfoError> {
        let url = format!("{}/rfo/sites", self.base_url);

        let response = self
            .execute_with_retry(|client| {
                let url = url.clone();
                let client = client.clone();
                async move { client.get(&url).send().await }
            })
            .await?;

        let status = response.status();
        if !status.is_success() {
            let body_text = response.text().await.unwrap_or_default();
            return Err(RfoError::Server {
                status: status.as_u16(),
                body: body_text,
            });
        }

        let sites: Vec<serde_json::Value> = response.json().await?;
        Ok(sites)
    }

    /// Health check against the engine.
    pub async fn health_check(&self) -> Result<serde_json::Value, RfoError> {
        let url = format!("{}/rfo/health", self.base_url);

        let response = self.http.get(&url).send().await?;

        let status = response.status();
        if !status.is_success() {
            let body_text = response.text().await.unwrap_or_default();
            return Err(RfoError::Server {
                status: status.as_u16(),
                body: body_text,
            });
        }

        let health: serde_json::Value = response.json().await?;
        Ok(health)
    }

    // ── Batch Operations ────────────────────────────────────────────────

    /// Handshake with multiple domains concurrently.
    /// Returns (domain, Result) pairs — partial failures are isolated.
    pub async fn batch_handshake(
        &self,
        domains: &[&str],
        coordinates: HashMap<String, String>,
        payload_type: PayloadType,
    ) -> Vec<(String, Result<HandshakeResponse, RfoError>)> {
        let mut handles = Vec::with_capacity(domains.len());

        for domain in domains {
            let client = self.clone();
            let coords = coordinates.clone();
            let domain = domain.to_string();
            let ptype = payload_type.clone();

            handles.push(tokio::spawn(async move {
                let result = client.handshake(&domain, coords, ptype).await;
                (domain, result)
            }));
        }

        let mut results = Vec::with_capacity(handles.len());
        for handle in handles {
            match handle.await {
                Ok(result) => results.push(result),
                Err(e) => {
                    tracing::error!("Batch handshake task panicked: {}", e);
                }
            }
        }

        results
    }

    // ── Telemetry ───────────────────────────────────────────────────────

    /// Fetch the telemetry dashboard report from the engine.
    pub async fn get_telemetry(&self) -> Result<serde_json::Value, RfoError> {
        let url = format!("{}/rfo/telemetry", self.base_url);

        let response = self.http.get(&url).send().await?;

        let status = response.status();
        if !status.is_success() {
            let body_text = response.text().await.unwrap_or_default();
            return Err(RfoError::Server {
                status: status.as_u16(),
                body: body_text,
            });
        }

        let report: serde_json::Value = response.json().await?;
        Ok(report)
    }

    /// Server-side batch handshake (single request to the engine).
    pub async fn server_batch_handshake(
        &self,
        domains: &[&str],
        coordinates: HashMap<String, String>,
        payload_type: PayloadType,
    ) -> Result<Vec<serde_json::Value>, RfoError> {
        let url = format!("{}/rfo/batch-handshake", self.base_url);

        let body = serde_json::json!({
            "domains": domains,
            "coordinates": coordinates,
            "requested_payload": payload_type,
        });

        let response = self
            .execute_with_retry(|client| {
                let url = url.clone();
                let body = body.clone();
                let client = client.clone();
                async move {
                    client
                        .post(&url)
                        .header("Content-Type", "application/json")
                        .body(body.to_string())
                        .send()
                        .await
                }
            })
            .await?;

        let status = response.status();
        if !status.is_success() {
            let body_text = response.text().await.unwrap_or_default();
            return Err(RfoError::Server {
                status: status.as_u16(),
                body: body_text,
            });
        }

        let results: Vec<serde_json::Value> = response.json().await?;
        Ok(results)
    }

    // ── Internal ────────────────────────────────────────────────────────

    /// Execute an HTTP operation with exponential backoff retry.
    async fn execute_with_retry<F, Fut>(&self, operation: F) -> Result<reqwest::Response, RfoError>
    where
        F: Fn(Client) -> Fut,
        Fut: std::future::Future<Output = Result<reqwest::Response, reqwest::Error>>,
    {
        let mut last_error = String::new();
        let mut backoff = self.retry.initial_backoff;

        for attempt in 1..=self.retry.max_attempts {
            match operation(self.http.clone()).await {
                Ok(response) => {
                    let status = response.status();
                    // Retry on 5xx server errors
                    if status.is_server_error() && attempt < self.retry.max_attempts {
                        let body_text = response.text().await.unwrap_or_default();
                        last_error = format!("server {}: {}", status, body_text);
                        tokio::time::sleep(backoff).await;
                        backoff = Duration::from_secs_f64(
                            (backoff.as_secs_f64() * self.retry.backoff_multiplier)
                                .min(self.retry.max_backoff.as_secs_f64()),
                        );
                        continue;
                    }
                    return Ok(response);
                }
                Err(e) => {
                    let is_retryable =
                        e.is_timeout() || e.is_connect() || e.is_request();
                    if is_retryable && attempt < self.retry.max_attempts {
                        last_error = e.to_string();
                        tokio::time::sleep(backoff).await;
                        backoff = Duration::from_secs_f64(
                            (backoff.as_secs_f64() * self.retry.backoff_multiplier)
                                .min(self.retry.max_backoff.as_secs_f64()),
                        );
                        continue;
                    }
                    return Err(e.into());
                }
            }
        }

        Err(RfoError::RetryExhausted {
            attempts: self.retry.max_attempts,
            last_error,
        })
    }
}

// ── Convenience builder ────────────────────────────────────────────────────

pub struct RfoClientBuilder {
    base_url: String,
    retry: RetryConfig,
    timeout: Duration,
    connect_timeout: Duration,
}

impl RfoClientBuilder {
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.to_string(),
            retry: RetryConfig::default(),
            timeout: Duration::from_secs(10),
            connect_timeout: Duration::from_secs(3),
        }
    }

    pub fn retry(mut self, retry: RetryConfig) -> Self {
        self.retry = retry;
        self
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = timeout;
        self
    }

    pub fn build(self) -> Result<RfoClient, RfoError> {
        let http = Client::builder()
            .timeout(self.timeout)
            .connect_timeout(self.connect_timeout)
            .pool_max_idle_per_host(10)
            .build()
            .map_err(|e| RfoError::Network(e.to_string()))?;

        Ok(RfoClient {
            base_url: self.base_url.trim_end_matches('/').to_string(),
            http,
            retry: self.retry,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_creation() {
        let client = RfoClient::new("http://localhost:3000");
        assert_eq!(client.base_url, "http://localhost:3000");
    }

    #[test]
    fn test_client_strips_trailing_slash() {
        let client = RfoClient::new("http://localhost:3000/");
        assert_eq!(client.base_url, "http://localhost:3000");
    }

    #[test]
    fn test_retry_config_default() {
        let config = RetryConfig::default();
        assert_eq!(config.max_attempts, 3);
        assert_eq!(config.initial_backoff, Duration::from_millis(200));
        assert_eq!(config.max_backoff, Duration::from_secs(5));
        assert!((config.backoff_multiplier - 2.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_builder() {
        let client = RfoClientBuilder::new("http://localhost:3000")
            .timeout(Duration::from_secs(5))
            .retry(RetryConfig {
                max_attempts: 5,
                ..Default::default()
            })
            .build()
            .unwrap();
        assert_eq!(client.base_url, "http://localhost:3000");
        assert_eq!(client.retry.max_attempts, 5);
    }

    #[test]
    fn test_error_display() {
        let err = RfoError::Server {
            status: 404,
            body: "not found".to_string(),
        };
        assert_eq!(err.to_string(), "server error 404: not found");

        let err = RfoError::RetryExhausted {
            attempts: 3,
            last_error: "connection refused".to_string(),
        };
        assert!(err.to_string().contains("retry exhausted after 3 attempts"));
    }
}
