use chrono::Utc;
use serde::{Deserialize, Serialize};

    use crate::compiler::{compile_doc, compile_mdoc, calculate_quality_score};
    use crate::parser::parse_html;
    use crate::rfo_protocol::{FullDocPayload, MiniDocPayload};
    use crate::domain::RfoDomain;

// ── Document Pipeline ────────────────────────────────────────────────────────
//
// The document pipeline converts website content into RFO-optimized formats:
//
//   Website HTML → Parser → ParsedContent → Compiler → .doc/.mdoc
//
// For .opt domains, the pipeline also:
//   - Generates SEO metadata
//   - Creates GEO-optimized content
//   - Builds AEO Q&A pairs
//   - Produces JSON-LD structured data
//   - Calculates quality scores
//
// The pipeline supports:
//   - Single page compilation
//   - Batch compilation (entire websites)
//   - Incremental updates (only changed pages)
//   - Quality monitoring (track score changes over time)

/// Configuration for the document pipeline.
#[derive(Debug, Clone)]
pub struct PipelineConfig {
    /// Maximum pages to compile in a batch
    pub max_batch_size: usize,
    /// Whether to generate .mdoc for all pages
    pub generate_mdoc: bool,
    /// Whether to generate .doc for all pages
    pub generate_doc: bool,
    /// Whether to generate JSON-LD structured data
    pub generate_structured_data: bool,
    /// Whether to generate FAQ schema
    pub generate_faq_schema: bool,
    /// Maximum Q&A pairs per page
    pub max_qa_pairs: usize,
    /// Whether to calculate quality scores
    pub calculate_quality: bool,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        PipelineConfig {
            max_batch_size: 100,
            generate_mdoc: true,
            generate_doc: true,
            generate_structured_data: true,
            generate_faq_schema: true,
            max_qa_pairs: 20,
            calculate_quality: true,
        }
    }
}

/// A compiled document package for a single page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompiledPage {
    /// The domain this page belongs to
    pub domain: String,
    /// The page URL
    pub url: String,
    /// Page title
    pub title: String,
    /// The .mdoc payload (if generated)
    pub mdoc: Option<MiniDocPayload>,
    /// The .doc payload (if generated)
    pub doc: Option<FullDocPayload>,
    /// Quality score (0-100)
    pub quality_score: u32,
    /// Token count for .mdoc
    pub token_count: usize,
    /// Number of Q&A pairs
    pub qa_pair_count: usize,
    /// JSON-LD structured data (if generated)
    pub structured_data: Option<serde_json::Value>,
    /// FAQ schema (if generated)
    pub faq_schema: Option<serde_json::Value>,
    /// Compilation timestamp
    pub compiled_at: String,
    /// Processing time in milliseconds
    pub processing_time_ms: u64,
}

/// A batch compilation result for an entire website.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompiledSite {
    /// The domain
    pub domain: String,
    /// Whether this is an .opt domain
    pub is_opt: bool,
    /// All compiled pages
    pub pages: Vec<CompiledPage>,
    /// Total pages compiled
    pub total_pages: usize,
    /// Average quality score
    pub avg_quality_score: f64,
    /// Total token count across all .mdoc files
    pub total_tokens: usize,
    /// Total Q&A pairs
    pub total_qa_pairs: usize,
    /// Site-wide structured data
    pub site_structured_data: Option<serde_json::Value>,
    /// Compilation timestamp
    pub compiled_at: String,
    /// Total processing time in milliseconds
    pub total_processing_time_ms: u64,
}

/// Statistics for a compiled site.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SiteStats {
    /// Total pages
    pub total_pages: usize,
    /// Pages with .mdoc
    pub pages_with_mdoc: usize,
    /// Pages with .doc
    pub pages_with_doc: usize,
    /// Average quality score
    pub avg_quality_score: f64,
    /// Min quality score
    pub min_quality_score: u32,

    pub max_quality_score: u32,
    /// Total tokens
    pub total_tokens: usize,
    /// Total Q&A pairs
    pub total_qa_pairs: usize,
    /// Pages with structured data
    pub pages_with_structured_data: usize,
    /// Pages with FAQ schema
    pub pages_with_faq_schema: usize,
}

// ── Pipeline Implementation ──────────────────────────────────────────────────

/// The document pipeline for compiling websites into RFO format.
pub struct DocumentPipeline {
    config: PipelineConfig,
}

impl DocumentPipeline {
    /// Create a new pipeline with default configuration.
    pub fn new() -> Self {
        DocumentPipeline {
            config: PipelineConfig::default(),
        }
    }

    /// Create a new pipeline with custom configuration.
    pub fn with_config(config: PipelineConfig) -> Self {
        DocumentPipeline { config }
    }

    /// Compile a single HTML page into RFO format.
    pub fn compile_page(
        &self,
        domain: &str,
        url: &str,
        html: &str,
    ) -> CompiledPage {
        let start = std::time::Instant::now();

        // Parse HTML
        let parsed = parse_html(html);

        // Compile .mdoc
        let mdoc = if self.config.generate_mdoc {
            Some(compile_mdoc(&parsed))
        } else {
            None
        };

        // Compile .doc
        let doc = if self.config.generate_doc {
            Some(compile_doc(&parsed, domain))
        } else {
            None
        };

        // Calculate quality score
        let quality_score = if self.config.calculate_quality {
            match (&mdoc, &doc) {
                (Some(m), Some(d)) => calculate_quality_score(m, d),
                _ => 0,
            }
        } else {
            0
        };

        // Generate structured data
        let structured_data = if self.config.generate_structured_data {
            Some(self.generate_structured_data(domain, &parsed.title, url))
        } else {
            None
        };

        // Generate FAQ schema
        let faq_schema = if self.config.generate_faq_schema {
            mdoc.as_ref().map(|m| {
                let qa_vec: Vec<(String, String)> = m.qa_pairs.iter()
                    .map(|qa| (qa.question.clone(), qa.answer.clone()))
                    .collect();
                self.generate_faq_schema(&qa_vec)
            })
        } else {
            None
        };

        let token_count = mdoc.as_ref().map(|m| m.token_count).unwrap_or(0);
        let qa_pair_count = mdoc.as_ref().map(|m| m.qa_pairs.len()).unwrap_or(0);

        let processing_time_ms = start.elapsed().as_millis() as u64;

        CompiledPage {
            domain: domain.to_string(),
            url: url.to_string(),
            title: parsed.title,
            mdoc,
            doc,
            quality_score,
            token_count,
            qa_pair_count,
            structured_data,
            faq_schema,
            compiled_at: Utc::now().to_rfc3339(),
            processing_time_ms,
        }
    }

    /// Compile multiple pages for a domain.
    pub fn compile_site(
        &self,
        domain: &str,
        pages: &[(String, String)], // (url, html) pairs
    ) -> CompiledSite {
        let start = std::time::Instant::now();
        let is_opt = RfoDomain::parse(domain)
            .map(|d| d.is_opt())
            .unwrap_or(false);

        let compiled_pages: Vec<CompiledPage> = pages
            .iter()
            .take(self.config.max_batch_size)
            .map(|(url, html)| self.compile_page(domain, url, html))
            .collect();

        let total_pages = compiled_pages.len();
        let avg_quality_score = if total_pages > 0 {
            compiled_pages.iter().map(|p| p.quality_score as f64).sum::<f64>() / total_pages as f64
        } else {
            0.0
        };
        let total_tokens = compiled_pages.iter().map(|p| p.token_count).sum();
        let total_qa_pairs = compiled_pages.iter().map(|p| p.qa_pair_count).sum();

        // Generate site-wide structured data
        let site_structured_data = if is_opt && self.config.generate_structured_data {
            Some(self.generate_site_structured_data(domain, total_pages))
        } else {
            None
        };

        let total_processing_time_ms = start.elapsed().as_millis() as u64;

        CompiledSite {
            domain: domain.to_string(),
            is_opt,
            pages: compiled_pages,
            total_pages,
            avg_quality_score,
            total_tokens,
            total_qa_pairs,
            site_structured_data,
            compiled_at: Utc::now().to_rfc3339(),
            total_processing_time_ms,
        }
    }

    /// Calculate statistics for a compiled site.
    pub fn calculate_stats(&self, site: &CompiledSite) -> SiteStats {
        let total_pages = site.pages.len();
        let pages_with_mdoc = site.pages.iter().filter(|p| p.mdoc.is_some()).count();
        let pages_with_doc = site.pages.iter().filter(|p| p.doc.is_some()).count();

        let scores: Vec<u32> = site.pages.iter().map(|p| p.quality_score).collect();
        let avg_quality_score = if total_pages > 0 {
            scores.iter().map(|s| *s as f64).sum::<f64>() / total_pages as f64
        } else {
            0.0
        };
        let min_quality_score = scores.iter().copied().min().unwrap_or(0);
        let max_quality_score = scores.iter().copied().max().unwrap_or(100);

        let total_tokens = site.pages.iter().map(|p| p.token_count).sum();
        let total_qa_pairs = site.pages.iter().map(|p| p.qa_pair_count).sum();
        let pages_with_structured_data = site.pages.iter().filter(|p| p.structured_data.is_some()).count();
        let pages_with_faq_schema = site.pages.iter().filter(|p| p.faq_schema.is_some()).count();

        SiteStats {
            total_pages,
            pages_with_mdoc,
            pages_with_doc,
            avg_quality_score,
            min_quality_score,
            max_quality_score,
            total_tokens,
            total_qa_pairs,
            pages_with_structured_data,
            pages_with_faq_schema,
        }
    }

    // ── Helper Methods ─────────────────────────────────────────────────

    fn generate_structured_data(
        &self,
        domain: &str,
        title: &str,
        url: &str,
    ) -> serde_json::Value {
        serde_json::json!({
            "@context": "https://schema.org",
            "@type": "WebPage",
            "name": title,
            "url": url,
            "isPartOf": {
                "@type": "WebSite",
                "name": format!("{}.opt", domain),
                "url": format!("https://{}.opt", domain)
            },
            "publisher": {
                "@type": "Organization",
                "name": format!("{}.opt", domain)
            }
        })
    }

    fn generate_faq_schema(&self, qa_pairs: &[(String, String)]) -> serde_json::Value {
        let items: Vec<serde_json::Value> = qa_pairs
            .iter()
            .take(self.config.max_qa_pairs)
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

    fn generate_site_structured_data(&self, domain: &str, total_pages: usize) -> serde_json::Value {
        serde_json::json!({
            "@context": "https://schema.org",
            "@type": "WebSite",
            "name": format!("{}.opt", domain),
            "url": format!("https://{}.opt", domain),
            "description": format!("AI-optimized documentation for {}", domain),
            "potentialAction": {
                "@type": "SearchAction",
                "target": format!("https://{}.opt/search?q={{search_term_string}}", domain),
                "query-input": "required name=search_term_string"
            },
            "numberOfPages": total_pages,
            "inLanguage": "en",
            "isAccessibleForFree": true
        })
    }
}

impl Default for DocumentPipeline {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_HTML: &str = r#"
<!DOCTYPE html>
<html>
<head><title>Test Page</title></head>
<body>
<h1>Test Page</h1>
<p>This is a test page for the document pipeline.</p>
<p>It contains basic content for compilation.</p>
<h2>What is RFO?</h2>
<p>RFO is a protocol for AI agents to consume web content efficiently.</p>
<h2>How does it work?</h2>
<p>RFO compiles websites into structured, verified, token-optimized payloads.</p>
<table>
<tr><th>Feature</th><th>Status</th></tr>
<tr><td>Handshake</td><td>Active</td></tr>
<tr><td>Cache</td><td>Ready</td></tr>
</table>
</body>
</html>
"#;

    #[test]
    fn test_compile_page() {
        let pipeline = DocumentPipeline::new();
        let page = pipeline.compile_page("example.com", "https://example.com", TEST_HTML);

        assert_eq!(page.domain, "example.com");
        assert!(!page.title.is_empty());
        assert!(page.mdoc.is_some());
        assert!(page.doc.is_some());
        assert!(page.quality_score > 0);
        assert!(page.token_count > 0);
        assert!(page.qa_pair_count > 0);
        assert!(page.processing_time_ms >= 0);
    }

    #[test]
    fn test_compile_opt_page() {
        let pipeline = DocumentPipeline::new();
        let page = pipeline.compile_page("mysite.opt", "https://mysite.opt", TEST_HTML);

        assert!(page.structured_data.is_some());
        assert!(page.faq_schema.is_some());
    }

    #[test]
    fn test_compile_site() {
        let pipeline = DocumentPipeline::new();
        let pages = vec![
            ("https://example.com".to_string(), TEST_HTML.to_string()),
            ("https://example.com/about".to_string(), TEST_HTML.to_string()),
        ];
        let site = pipeline.compile_site("example.com", &pages);

        assert_eq!(site.domain, "example.com");
        assert_eq!(site.total_pages, 2);
        assert!(site.avg_quality_score > 0.0);
        assert!(site.total_tokens > 0);
    }

    #[test]
    fn test_calculate_stats() {
        let pipeline = DocumentPipeline::new();
        let pages = vec![
            ("https://example.com".to_string(), TEST_HTML.to_string()),
        ];
        let site = pipeline.compile_site("example.com", &pages);
        let stats = pipeline.calculate_stats(&site);

        assert_eq!(stats.total_pages, 1);
        assert_eq!(stats.pages_with_mdoc, 1);
        assert_eq!(stats.pages_with_doc, 1);
        assert!(stats.avg_quality_score > 0.0);
    }

    #[test]
    fn test_pipeline_config() {
        let config = PipelineConfig {
            max_batch_size: 5,
            generate_mdoc: false,
            generate_doc: true,
            ..Default::default()
        };
        let pipeline = DocumentPipeline::with_config(config);
        let page = pipeline.compile_page("example.com", "https://example.com", TEST_HTML);

        assert!(page.mdoc.is_none());
        assert!(page.doc.is_some());
    }

    #[test]
    fn test_max_batch_size() {
        let config = PipelineConfig {
            max_batch_size: 2,
            ..Default::default()
        };
        let pipeline = DocumentPipeline::with_config(config);
        let pages = vec![
            ("https://a.com".to_string(), TEST_HTML.to_string()),
            ("https://b.com".to_string(), TEST_HTML.to_string()),
            ("https://c.com".to_string(), TEST_HTML.to_string()),
        ];
        let site = pipeline.compile_site("example.com", &pages);

        assert_eq!(site.total_pages, 2); // Limited by max_batch_size
    }
}
