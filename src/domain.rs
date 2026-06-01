use serde::{Deserialize, Serialize};
use std::fmt;

// ── .opt Domain Types ────────────────────────────────────────────────────────
//
// The .opt domain is a purpose-built TLD for AI-optimized content delivery.
// It natively integrates with the RFO protocol for:
//   - SEO (Search Engine Optimization) — structured data, metadata
//   - GEO (Generative Engine Optimization) — LLM-ready content
//   - AEO (Answer Engine Optimization) — Q&A pairs, direct answers
//
// Every .opt domain automatically provides:
//   - .doc (Full Knowledge Document) — deep content with verification
//   - .mdoc (Mini Document) — token-optimized for LLM context windows
//   - Site ID — cryptographic identity via HMAC-SHA256
//   - Quality Score — automated 0-100 scoring
//   - Coordinates — semantic metadata for discovery

/// Supported TLD types in the RFO protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Tld {
    /// Standard domains (example.com, docs.rs)
    Standard,
    /// .opt domains — AI-optimized content delivery
    Opt,
}

impl fmt::Display for Tld {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Tld::Standard => write!(f, ""),
            Tld::Opt => write!(f, ".opt"),
        }
    }
}

/// A parsed RFO domain with TLD information.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RfoDomain {
    /// The full domain string (e.g., "example.com" or "mysite.opt")
    pub full: String,
    /// The domain name without TLD (e.g., "example" or "mysite")
    pub name: String,
    /// The TLD type
    pub tld: Tld,
    /// Whether this is a subdomain (e.g., "api.example.opt")
    pub is_subdomain: bool,
    /// Subdomain prefix (if any, e.g., "api" in "api.example.opt")
    pub subdomain: Option<String>,
}

impl RfoDomain {
    /// Parse a domain string into an RfoDomain.
    ///
    /// Supports:
    ///   - Standard: "example.com", "docs.rs"
    ///   - .opt: "mysite.opt", "blog.mysite.opt"
    pub fn parse(domain: &str) -> Result<Self, DomainError> {
        let domain_str = domain.trim().to_lowercase();
        if domain_str.is_empty() {
            return Err(DomainError::Empty);
        }

        // Check for .opt TLD
        if domain_str.ends_with(".opt") {
            let without_tld_len = domain_str.len() - 4;
            let without_tld = domain_str[..without_tld_len].to_string();
            let parts: Vec<&str> = without_tld.split('.').collect();

            match parts.len() {
                0 => Err(DomainError::InvalidFormat),
                1 => Ok(RfoDomain {
                    full: domain_str,
                    name: parts[0].to_string(),
                    tld: Tld::Opt,
                    is_subdomain: false,
                    subdomain: None,
                }),
                _ => Ok(RfoDomain {
                    full: domain_str,
                    name: parts.last().unwrap().to_string(),
                    tld: Tld::Opt,
                    is_subdomain: true,
                    subdomain: Some(parts[..parts.len() - 1].join(".")),
                }),
            }
        } else {
            // Standard domain
            let parts: Vec<&str> = domain_str.split('.').collect();
            if parts.len() < 2 {
                return Err(DomainError::InvalidFormat);
            }

            let name = parts[parts.len() - 2].to_string();
            let is_subdomain = parts.len() > 2;
            let subdomain = if is_subdomain {
                Some(parts[..parts.len() - 2].join("."))
            } else {
                None
            };

            Ok(RfoDomain {
                full: domain_str,
                name,
                tld: Tld::Standard,
                is_subdomain,
                subdomain,
            })
        }
    }

    /// Check if this is an .opt domain.
    pub fn is_opt(&self) -> bool {
        self.tld == Tld::Opt
    }

    /// Get the canonical site ID input for .opt domains.
    /// For .opt domains, this includes the TLD for unique identification.
    pub fn site_id_input(&self) -> &str {
        &self.full
    }

    /// Get the display URL for this domain.
    pub fn display_url(&self) -> String {
        format!("https://{}", self.full)
    }

    /// Get SEO-friendly URL path for .doc files.
    pub fn doc_path(&self) -> String {
        format!("/rfo/doc/{}", self.full)
    }

    /// Get SEO-friendly URL path for .mdoc files.
    pub fn mdoc_path(&self) -> String {
        format!("/rfo/mdoc/{}", self.full)
    }

    /// Get the binary stream path for .opt domains.
    pub fn stream_path(&self) -> String {
        format!("/rfo/stream/{}", self.full)
    }

    /// Validate the domain format.
    pub fn validate(&self) -> Result<(), DomainError> {
        if self.name.is_empty() {
            return Err(DomainError::EmptyName);
        }

        // Domain names can only contain alphanumeric and hyphens
        if !self.name.chars().all(|c| c.is_alphanumeric() || c == '-') {
            return Err(DomainError::InvalidCharacters);
        }

        // Cannot start or end with hyphen
        if self.name.starts_with('-') || self.name.ends_with('-') {
            return Err(DomainError::InvalidHyphen);
        }

        // Max 63 characters per label
        if self.name.len() > 63 {
            return Err(DomainError::NameTooLong);
        }

        Ok(())
    }
}

impl fmt::Display for RfoDomain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.full)
    }
}

/// Errors during domain parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DomainError {
    Empty,
    InvalidFormat,
    EmptyName,
    InvalidCharacters,
    InvalidHyphen,
    NameTooLong,
}

impl fmt::Display for DomainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DomainError::Empty => write!(f, "domain is empty"),
            DomainError::InvalidFormat => write!(f, "invalid domain format"),
            DomainError::EmptyName => write!(f, "domain name is empty"),
            DomainError::InvalidCharacters => write!(f, "domain contains invalid characters"),
            DomainError::InvalidHyphen => write!(f, "domain cannot start or end with hyphen"),
            DomainError::NameTooLong => write!(f, "domain name exceeds 63 characters"),
        }
    }
}

impl std::error::Error for DomainError {}

// ── .opt Domain Metadata ─────────────────────────────────────────────────────

/// Metadata specific to .opt domains.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptMetadata {
    /// Domain name (without .opt)
    pub domain: String,
    /// SEO metadata
    pub seo: SeoMetadata,
    /// GEO metadata (generative engine optimization)
    pub geo: GeoMetadata,
    /// AEO metadata (answer engine optimization)
    pub aeo: AeoMetadata,
    /// Domain registration timestamp
    pub registered_at: String,
    /// Domain expiry timestamp
    pub expires_at: Option<String>,
    /// Whether the domain is verified
    pub verified: bool,
}

/// SEO metadata for .opt domains.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeoMetadata {
    /// Page title
    pub title: String,
    /// Meta description
    pub description: String,
    /// Keywords
    pub keywords: Vec<String>,
    /// Canonical URL
    pub canonical_url: String,
    /// Open Graph tags
    pub og_title: Option<String>,
    pub og_description: Option<String>,
    pub og_image: Option<String>,
    /// Structured data (JSON-LD)
    pub structured_data: Option<serde_json::Value>,
}

/// GEO metadata for generative engine optimization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeoMetadata {
    /// Whether the domain is LLM-friendly
    pub llm_friendly: bool,
    /// Content type (documentation, blog, api, etc.)
    pub content_type: String,
    /// Primary language
    pub language: String,
    /// Content categories
    pub categories: Vec<String>,
    /// Whether the domain provides direct answers
    pub direct_answers: bool,
    /// Whether the domain has structured data
    pub structured_data: bool,
}

/// AEO metadata for answer engine optimization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AeoMetadata {
    /// Whether the domain has Q&A pairs
    pub has_qa_pairs: bool,
    /// Number of Q&A pairs available
    pub qa_pair_count: u32,
    /// Whether the domain has featured snippets
    pub featured_snippets: bool,
    /// Whether the domain has FAQ schema
    pub faq_schema: bool,
    /// Whether the domain provides direct answers
    pub direct_answers: bool,
    /// Answer confidence score (0-100)
    pub answer_confidence: u8,
}

impl OptMetadata {
    /// Create default .opt metadata for a domain.
    pub fn new(domain: &str) -> Self {
        OptMetadata {
            domain: domain.to_string(),
            seo: SeoMetadata {
                title: format!("{} - AI Optimized Domain", domain),
                description: format!("{} is an AI-optimized domain powered by the RFO Protocol", domain),
                keywords: vec![
                    "AI".to_string(),
                    "RFO".to_string(),
                    "optimized".to_string(),
                    domain.to_string(),
                ],
                canonical_url: format!("https://{}.opt", domain),
                og_title: None,
                og_description: None,
                og_image: None,
                structured_data: None,
            },
            geo: GeoMetadata {
                llm_friendly: true,
                content_type: "documentation".to_string(),
                language: "en".to_string(),
                categories: vec!["technology".to_string()],
                direct_answers: true,
                structured_data: true,
            },
            aeo: AeoMetadata {
                has_qa_pairs: true,
                qa_pair_count: 0,
                featured_snippets: true,
                faq_schema: true,
                direct_answers: true,
                answer_confidence: 80,
            },
            registered_at: chrono::Utc::now().to_rfc3339(),
            expires_at: None,
            verified: false,
        }
    }

    /// Generate JSON-LD structured data for the domain.
    pub fn to_json_ld(&self) -> serde_json::Value {
        serde_json::json!({
            "@context": "https://schema.org",
            "@type": "WebSite",
            "name": self.seo.title,
            "description": self.seo.description,
            "url": self.seo.canonical_url,
            "potentialAction": {
                "@type": "SearchAction",
                "target": format!("{}?q={{search_term_string}}", self.seo.canonical_url),
                "query-input": "required name=search_term_string"
            },
            "publisher": {
                "@type": "Organization",
                "name": format!("{}.opt", self.domain),
                "url": self.seo.canonical_url
            }
        })
    }

    /// Generate FAQ schema for AEO.
    pub fn faq_schema(&self, qa_pairs: &[(String, String)]) -> serde_json::Value {
        let items: Vec<serde_json::Value> = qa_pairs
            .iter()
            .map(|(q, a)| {
                serde_json::json!({
                    "@type": "Question",
                    "name": q,
                    "acceptedAnswer": {
                        "@type": "Answer",
                        "text": a
                    }
                })
            })
            .collect();

        serde_json::json!({
            "@context": "https://schema.org",
            "@type": "FAQPage",
            "mainEntity": items
        })
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_standard_domain() {
        let d = RfoDomain::parse("example.com").unwrap();
        assert_eq!(d.name, "example");
        assert_eq!(d.tld, Tld::Standard);
        assert!(!d.is_opt());
        assert!(!d.is_subdomain);
    }

    #[test]
    fn test_parse_opt_domain() {
        let d = RfoDomain::parse("mysite.opt").unwrap();
        assert_eq!(d.name, "mysite");
        assert_eq!(d.tld, Tld::Opt);
        assert!(d.is_opt());
        assert!(!d.is_subdomain);
    }

    #[test]
    fn test_parse_opt_subdomain() {
        let d = RfoDomain::parse("api.mysite.opt").unwrap();
        assert_eq!(d.name, "mysite");
        assert!(d.is_opt());
        assert!(d.is_subdomain);
        assert_eq!(d.subdomain.as_deref(), Some("api"));
    }

    #[test]
    fn test_parse_standard_subdomain() {
        let d = RfoDomain::parse("api.example.com").unwrap();
        assert_eq!(d.name, "example");
        assert!(!d.is_opt());
        assert!(d.is_subdomain);
    }

    #[test]
    fn test_empty_domain() {
        assert_eq!(RfoDomain::parse(""), Err(DomainError::Empty));
    }

    #[test]
    fn test_invalid_format() {
        assert_eq!(RfoDomain::parse("nodot"), Err(DomainError::InvalidFormat));
    }

    #[test]
    fn test_display() {
        let d = RfoDomain::parse("mysite.opt").unwrap();
        assert_eq!(d.to_string(), "mysite.opt");
    }

    #[test]
    fn test_doc_path() {
        let d = RfoDomain::parse("mysite.opt").unwrap();
        assert_eq!(d.doc_path(), "/rfo/doc/mysite.opt");
    }

    #[test]
    fn test_mdoc_path() {
        let d = RfoDomain::parse("mysite.opt").unwrap();
        assert_eq!(d.mdoc_path(), "/rfo/mdoc/mysite.opt");
    }

    #[test]
    fn test_validate_valid() {
        let d = RfoDomain::parse("mysite.opt").unwrap();
        assert!(d.validate().is_ok());
    }

    #[test]
    fn test_validate_hyphen_start() {
        let mut d = RfoDomain::parse("mysite.opt").unwrap();
        d.name = "-mysite".to_string();
        assert_eq!(d.validate(), Err(DomainError::InvalidHyphen));
    }

    #[test]
    fn test_validate_too_long() {
        let mut d = RfoDomain::parse("mysite.opt").unwrap();
        d.name = "a".repeat(64);
        assert_eq!(d.validate(), Err(DomainError::NameTooLong));
    }

    #[test]
    fn test_opt_metadata_new() {
        let m = OptMetadata::new("testsite");
        assert_eq!(m.domain, "testsite");
        assert!(m.geo.llm_friendly);
        assert!(m.aeo.has_qa_pairs);
    }

    #[test]
    fn test_json_ld() {
        let m = OptMetadata::new("testsite");
        let json = m.to_json_ld();
        assert_eq!(json["@type"], "WebSite");
        assert!(json["url"].as_str().unwrap().contains("testsite.opt"));
    }

    #[test]
    fn test_faq_schema() {
        let m = OptMetadata::new("testsite");
        let qa = vec![
            ("What is RFO?".to_string(), "A protocol for AI agents.".to_string()),
            ("What is .opt?".to_string(), "An AI-optimized TLD.".to_string()),
        ];
        let schema = m.faq_schema(&qa);
        assert_eq!(schema["@type"], "FAQPage");
        let entities = schema["mainEntity"].as_array().unwrap();
        assert_eq!(entities.len(), 2);
    }

    #[test]
    fn test_case_insensitive() {
        let d = RfoDomain::parse("MySite.OPT").unwrap();
        assert_eq!(d.full, "mysite.opt");
    }
}
