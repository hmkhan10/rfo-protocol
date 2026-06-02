use std::net::SocketAddr;
use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt, BufWriter};
use tokio::net::{TcpListener, TcpStream};

use crate::binary::{
    calculate_checksum, BinaryHeader, BinaryError, RFO_MAGIC, PROTOCOL_VERSION,
    TYPE_CORE_FILE, TYPE_ERROR, TYPE_HANDSHAKE, TYPE_RESOLVE_OPT,
    MAX_PAYLOAD_SIZE,
};
use crate::core_file::CoreFile;
use crate::opt_resolver::OptResolver;

#[derive(Debug, Clone)]
pub struct TransportConfig {
    pub bind_addr: SocketAddr,
    pub max_frame_size: usize,
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            bind_addr: "127.0.0.1:9000".parse().unwrap(),
            max_frame_size: MAX_PAYLOAD_SIZE,
        }
    }
}

pub struct NativeTransport {
    config: TransportConfig,
    resolver: Arc<OptResolver>,
}

impl NativeTransport {
    pub fn new(config: TransportConfig) -> Self {
        Self {
            config,
            resolver: Arc::new(OptResolver::new()),
        }
    }

    pub fn with_resolver(config: TransportConfig, resolver: Arc<OptResolver>) -> Self {
        Self { config, resolver }
    }

    pub fn resolver(&self) -> &Arc<OptResolver> {
        &self.resolver
    }

    pub async fn serve(&self) -> Result<(), Box<dyn std::error::Error>> {
        let listener = TcpListener::bind(self.config.bind_addr).await?;
        tracing::info!("RFO Native Transport listening on {}", self.config.bind_addr);

        loop {
            let (stream, addr) = listener.accept().await?;
            let resolver = self.resolver.clone();
            let max_size = self.config.max_frame_size;
            tokio::spawn(async move {
                if let Err(e) = handle_connection(stream, addr, resolver, max_size).await {
                    tracing::warn!("[{}] connection error: {}", addr, e);
                }
            });
        }
    }
}

async fn handle_connection(
    mut stream: TcpStream,
    addr: SocketAddr,
    resolver: Arc<OptResolver>,
    max_frame_size: usize,
) -> Result<(), BinaryError> {
    tracing::debug!("[{}] connected", addr);

    loop {
        let frame = read_frame(&mut stream, max_frame_size).await?;

        let response = match frame.payload_type {
            TYPE_HANDSHAKE => handle_handshake(&frame.payload).await,
            TYPE_RESOLVE_OPT => handle_resolve_opt(&frame.payload, &resolver),
            _ => Err(BinaryError::InvalidType(frame.payload_type)),
        };

        match response {
            Ok((resp_type, payload)) => {
                write_frame(&mut stream, resp_type, &payload).await?;
            }
            Err(e) => {
                let err_msg = format!("{}", e);
                write_frame(&mut stream, TYPE_ERROR, err_msg.as_bytes()).await?;
                if matches!(e, BinaryError::InvalidType(_)) {
                    break;
                }
            }
        }
    }

    tracing::debug!("[{}] disconnected", addr);
    Ok(())
}

struct RawFrame {
    payload_type: u8,
    payload: Vec<u8>,
}

async fn read_frame(
    stream: &mut TcpStream,
    max_size: usize,
) -> Result<RawFrame, BinaryError> {
    let mut header_buf = [0u8; 11];
    stream.read_exact(&mut header_buf).await.map_err(|e| {
        BinaryError::IoError(e.to_string())
    })?;

    let header = BinaryHeader::from_bytes(&header_buf)?;

    if header.magic != RFO_MAGIC {
        return Err(BinaryError::InvalidMagic);
    }
    if header.version != PROTOCOL_VERSION {
        return Err(BinaryError::UnsupportedVersion(header.version));
    }
    if header.length as usize > max_size {
        return Err(BinaryError::PayloadTooLarge(header.length as usize));
    }

    let len = header.length as usize;
    let mut payload = vec![0u8; len];
    if len > 0 {
        stream.read_exact(&mut payload).await.map_err(|e| {
            BinaryError::IoError(e.to_string())
        })?;
    }

    let mut crc_buf = [0u8; 4];
    stream.read_exact(&mut crc_buf).await.map_err(|e| {
        BinaryError::IoError(e.to_string())
    })?;
    let received_crc = u32::from_be_bytes(crc_buf);

    let computed_crc = calculate_checksum(&payload);
    if received_crc != computed_crc {
        return Err(BinaryError::ChecksumMismatch {
            expected: received_crc,
            actual: computed_crc,
        });
    }

    Ok(RawFrame {
        payload_type: header.payload_type,
        payload,
    })
}

async fn write_frame(
    stream: &mut TcpStream,
    payload_type: u8,
    payload: &[u8],
) -> Result<(), BinaryError> {
    let header = BinaryHeader {
        magic: RFO_MAGIC,
        version: PROTOCOL_VERSION,
        payload_type,
        length: payload.len() as u32,
    };

    let mut buf = Vec::with_capacity(11 + payload.len() + 4);
    buf.extend_from_slice(&header.to_bytes());
    buf.extend_from_slice(payload);

    let crc = calculate_checksum(payload);
    buf.extend_from_slice(&crc.to_be_bytes());

    stream.write_all(&buf).await.map_err(|e| {
        BinaryError::IoError(e.to_string())
    })?;
    stream.flush().await.map_err(|e| {
        BinaryError::IoError(e.to_string())
    })?;

    Ok(())
}

async fn handle_handshake(payload: &[u8]) -> Result<(u8, Vec<u8>), BinaryError> {
    #[derive(serde::Deserialize)]
    struct HandshakeReq {
        domain: String,
    }
    let req: HandshakeReq = serde_json::from_slice(payload)
        .map_err(|e| BinaryError::DeserializationError(e.to_string()))?;

    let resp = serde_json::json!({
        "status": "ok",
        "domain": req.domain,
        "protocol": "rfo-binary-v1",
        "timestamp": chrono::Utc::now().to_rfc3339(),
    });
    let resp_bytes = serde_json::to_vec(&resp)
        .map_err(|e| BinaryError::SerializationError(e.to_string()))?;

    Ok((TYPE_HANDSHAKE, resp_bytes))
}

fn handle_resolve_opt(payload: &[u8], resolver: &OptResolver) -> Result<(u8, Vec<u8>), BinaryError> {
    #[derive(serde::Deserialize)]
    struct ResolveReq {
        domain: String,
    }
    let req: ResolveReq = serde_json::from_slice(payload)
        .map_err(|e| BinaryError::DeserializationError(e.to_string()))?;

    let core_file = resolver.resolve(&req.domain)
        .map_err(|e| BinaryError::IoError(e.to_string()))?;

    let json = serde_json::to_vec(&core_file)
        .map_err(|e| BinaryError::SerializationError(e.to_string()))?;

    Ok((TYPE_CORE_FILE, json))
}

pub struct NativeClient {
    stream: BufWriter<TcpStream>,
}

impl NativeClient {
    pub async fn connect(addr: &str) -> Result<Self, BinaryError> {
        let stream = TcpStream::connect(addr).await
            .map_err(|e| BinaryError::IoError(e.to_string()))?;
        Ok(Self {
            stream: BufWriter::new(stream),
        })
    }

    pub async fn handshake(&mut self, domain: &str) -> Result<serde_json::Value, BinaryError> {
        let req = serde_json::json!({ "domain": domain });
        let req_bytes = serde_json::to_vec(&req)
            .map_err(|e| BinaryError::SerializationError(e.to_string()))?;

        write_frame(self.stream.get_mut(), TYPE_HANDSHAKE, &req_bytes).await?;

        let frame = read_frame(self.stream.get_mut(), MAX_PAYLOAD_SIZE).await?;
        let value: serde_json::Value = serde_json::from_slice(&frame.payload)
            .map_err(|e| BinaryError::DeserializationError(e.to_string()))?;
        Ok(value)
    }

    pub async fn resolve_opt(&mut self, domain: &str) -> Result<CoreFile, BinaryError> {
        let req = serde_json::json!({ "domain": domain });
        let req_bytes = serde_json::to_vec(&req)
            .map_err(|e| BinaryError::SerializationError(e.to_string()))?;

        write_frame(self.stream.get_mut(), TYPE_RESOLVE_OPT, &req_bytes).await?;

        let frame = read_frame(self.stream.get_mut(), MAX_PAYLOAD_SIZE).await?;

        if frame.payload_type == TYPE_ERROR {
            let err_msg = String::from_utf8_lossy(&frame.payload);
            return Err(BinaryError::IoError(err_msg.to_string()));
        }

        let core_file: CoreFile = serde_json::from_slice(&frame.payload)
            .map_err(|e| BinaryError::DeserializationError(e.to_string()))?;
        Ok(core_file)
    }

    pub async fn close(&mut self) -> Result<(), BinaryError> {
        self.stream.get_mut().shutdown().await
            .map_err(|e| BinaryError::IoError(e.to_string()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core_file::*;
    use crate::domain::{AeoMetadata, GeoMetadata, SeoMetadata};
    use std::collections::HashMap;

    fn mock_core_file(domain: &str) -> CoreFile {
        CoreFile {
            schema: crate::core_file::CORE_FILE_SCHEMA.to_string(),
            version: crate::core_file::CORE_FILE_VERSION.to_string(),
            compiled_at: chrono::Utc::now().to_rfc3339(),
            site: CoreSiteIdentity {
                site_id: format!("site_{}", domain),
                domain: domain.to_string(),
                is_opt: true,
                title: format!("Test {}", domain),
                description: format!("Description for {}", domain),
                coordinates: HashMap::new(),
                total_pages: 1,
                site_url: format!("https://{}", domain),
            },
            intelligence: CoreIntelligence {
                site_summary: format!("Intelligence for {}", domain),
                site_token_count: 1000,
                all_qa_pairs: vec![],
                topics: vec![CoreTopic {
                    name: "test".to_string(),
                    confidence: 0.8,
                    page_urls: vec![],
                }],
            },
            pages: vec![],
            quality: CoreQualityAggregate {
                overall: 85,
                avg_page: 85.0,
                best_page: format!("https://{}/page1", domain),
                best_score: 90,
                worst_page: format!("https://{}/page2", domain),
                worst_score: 80,
                total_tokens: 1000,
                total_qa_pairs: 10,
                pages_with_code: 1,
                pages_with_tables: 0,
                aeo_readiness: 65,
            },
            optimization: CoreOptimization {
                seo: SeoMetadata {
                    title: format!("Test {}", domain),
                    description: format!("A test site for {}", domain),
                    keywords: vec!["test".to_string()],
                    canonical_url: format!("https://{}/", domain),
                    og_title: None,
                    og_description: None,
                    og_image: None,
                    structured_data: None,
                },
                geo: GeoMetadata {
                    llm_friendly: true,
                    content_type: "website".to_string(),
                    language: "en".to_string(),
                    categories: vec!["test".to_string()],
                    direct_answers: true,
                    structured_data: true,
                },
                aeo: AeoMetadata {
                    has_qa_pairs: true,
                    qa_pair_count: 10,
                    featured_snippets: true,
                    faq_schema: false,
                    direct_answers: true,
                    answer_confidence: 85,
                },
                json_ld: None,
                faq_schema: None,
            },
            crypto: CoreCrypto {
                site_id_signature: "sig".to_string(),
                content_root_hash: "hash".to_string(),
                page_hashes: vec![],
                verified: true,
            },
        }
    }

    #[tokio::test]
    async fn test_roundtrip_resolve_opt() {
        let resolver = Arc::new(OptResolver::new());
        let cf = mock_core_file("test.opt");
        resolver.register("test.opt", cf.clone()).unwrap();

        let config = TransportConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            ..Default::default()
        };
        let _transport = NativeTransport::with_resolver(config, resolver.clone());

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let resolver_clone = resolver.clone();

        tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                let resolver = resolver_clone.clone();
                tokio::spawn(async move {
                    let _ = handle_connection(stream, addr, resolver, MAX_PAYLOAD_SIZE).await;
                });
            }
        });

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let mut client = NativeClient::connect(&addr.to_string()).await.unwrap();

        let handshake = client.handshake("test.opt").await.unwrap();
        assert_eq!(handshake["domain"], "test.opt");

        let resolved = client.resolve_opt("test.opt").await.unwrap();
        assert_eq!(resolved.site.domain, "test.opt");
        assert!(resolved.crypto.verified);

        client.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_resolve_nonexistent_opt() {
        let resolver = Arc::new(OptResolver::new());
        let config = TransportConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            ..Default::default()
        };
        let _transport = NativeTransport::with_resolver(config, resolver.clone());

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let resolver_clone = resolver.clone();

        tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                let resolver = resolver_clone.clone();
                tokio::spawn(async move {
                    let _ = handle_connection(stream, addr, resolver, MAX_PAYLOAD_SIZE).await;
                });
            }
        });

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let mut client = NativeClient::connect(&addr.to_string()).await.unwrap();
        let result = client.resolve_opt("nonexistent.opt").await;
        assert!(result.is_err());
        client.close().await.unwrap();
    }

    #[test]
    fn test_frame_roundtrip() {
        let payload = b"hello rfo binary transport";
        let header = BinaryHeader {
            magic: RFO_MAGIC,
            version: PROTOCOL_VERSION,
            payload_type: TYPE_HANDSHAKE,
            length: payload.len() as u32,
        };

        let header_bytes = header.to_bytes();
        let parsed = BinaryHeader::from_bytes(&header_bytes).unwrap();
        assert_eq!(parsed.magic, RFO_MAGIC);
        assert_eq!(parsed.version, PROTOCOL_VERSION);
        assert_eq!(parsed.payload_type, TYPE_HANDSHAKE);
        assert_eq!(parsed.length as usize, payload.len());

        let crc = calculate_checksum(payload);
        let computed = calculate_checksum(payload);
        assert_eq!(crc, computed);
    }
}
