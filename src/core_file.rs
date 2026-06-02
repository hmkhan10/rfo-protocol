use std::collections::HashMap;

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::compiler::{calculate_quality_score, compile_doc, compile_mdoc};
use crate::crypto::site_id;
use crate::domain::{AeoMetadata, GeoMetadata, OptMetadata, RfoDomain, SeoMetadata};
use crate::parser::{extract_coordinates, parse_html, parse_markdown};
use crate::pipeline::PipelineConfig;
use crate::rfo_protocol::{FullDocPayload, MiniDocPayload, QaPair};

pub const CORE_FILE_VERSION: &str = "1.0.0";
pub const CORE_FILE_SCHEMA: &str = "rfo-core-v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreFile {
    pub schema: String,
    pub version: String,
    pub compiled_at: String,
    pub site: CoreSiteIdentity,
    pub intelligence: CoreIntelligence,
    pub pages: Vec<CorePage>,
    pub quality: CoreQualityAggregate,
    pub optimization: CoreOptimization,
    pub crypto: CoreCrypto,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreSiteIdentity {
    pub site_id: String,
    pub domain: String,
    pub is_opt: bool,
    pub title: String,
    pub description: String,
    pub coordinates: HashMap<String, String>,
    pub total_pages: usize,
    pub site_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreIntelligence {
    pub site_summary: String,
    pub site_token_count: usize,
    pub all_qa_pairs: Vec<QaPair>,
    pub topics: Vec<CoreTopic>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreTopic {
    pub name: String,
    pub confidence: f64,
    pub page_urls: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorePage {
    pub url: String,
    pub path: String,
    pub title: String,
    pub depth: usize,
    pub quality_score: u32,
    pub token_count: usize,
    pub qa_pair_count: usize,
    pub doc: FullDocPayload,
    pub mdoc: MiniDocPayload,
    pub coordinates: HashMap<String, String>,
    pub structured_data: Option<serde_json::Value>,
    pub faq_schema: Option<serde_json::Value>,
    pub compiled_at: String,
    pub links_internal: Vec<String>,
    pub links_external: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreQualityAggregate {
    pub overall: u32,
    pub avg_page: f64,
    pub best_page: String,
    pub best_score: u32,
    pub worst_page: String,
    pub worst_score: u32,
    pub total_tokens: usize,
    pub total_qa_pairs: usize,
    pub pages_with_code: usize,
    pub pages_with_tables: usize,
    pub aeo_readiness: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreOptimization {
    pub seo: SeoMetadata,
    pub geo: GeoMetadata,
    pub aeo: AeoMetadata,
    pub json_ld: Option<serde_json::Value>,
    pub faq_schema: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreCrypto {
    pub site_id_signature: String,
    pub content_root_hash: String,
    pub page_hashes: Vec<CorePageHash>,
    pub verified: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorePageHash {
    pub url: String,
    pub sha256: String,
}

#[derive(Debug, Clone)]
pub struct CoreCompiler {
    config: PipelineConfig,
}

impl CoreCompiler {
    pub fn new() -> Self {
        CoreCompiler {
            config: PipelineConfig::default(),
        }
    }

    pub fn with_config(config: PipelineConfig) -> Self {
        CoreCompiler { config }
    }

    pub fn compile_from_pages(
        &self,
        domain: &str,
        pages: &[(String, String)],
    ) -> Result<CoreFile, String> {
        let domain_obj = RfoDomain::parse(domain).map_err(|e| format!("Domain error: {}", e))?;
        let is_opt = domain_obj.is_opt();
        let site_url = format!("https://{}", domain_obj.full);

        let site_id = site_id::generate_site_id(domain).map_err(|e| format!("Site ID error: {}", e))?;

        let mut core_pages = Vec::new();
        let mut all_qa = Vec::new();
        let mut all_topics: HashMap<String, Vec<String>> = HashMap::new();
        let mut site_paragaphs = Vec::new();
        let mut total_tokens = 0usize;
        let mut pages_with_code = 0usize;
        let mut pages_with_tables = 0usize;

        for (url, html) in pages {
            let is_markdown = url.ends_with(".md");
            let parsed = if is_markdown {
                parse_markdown(html)
            } else {
                parse_html(html)
            };

            let path = {
                let s = url
                    .trim_start_matches("https://")
                    .trim_start_matches("http://");
                if let Some(stripped) = s.strip_prefix(domain) {
                    stripped.trim_start_matches('/').to_string()
                } else {
                    s.to_string()
                }
            };

            let depth = path.matches('/').count().saturating_sub(1);

            let coordinates = extract_coordinates(&parsed);
            let page_coords = coordinates.clone();

            let page_mdoc = compile_mdoc(&parsed);
            let page_doc = compile_doc(&parsed, domain);
            let quality_score = calculate_quality_score(&page_mdoc, &page_doc);

            let structured_data = if is_opt {
                let opt = OptMetadata::new(&domain_obj.name);
                Some(opt.to_json_ld())
            } else {
                None
            };

            let faq_schema = if self.config.generate_faq_schema {
                let qa_vec: Vec<(String, String)> = page_mdoc.qa_pairs.iter()
                    .map(|qa| (qa.question.clone(), qa.answer.clone()))
                    .collect();
                if !qa_vec.is_empty() {
                    let opt = OptMetadata::new(&domain_obj.name);
                    Some(opt.faq_schema(&qa_vec))
                } else {
                    None
                }
            } else {
                None
            };

            if page_mdoc.qa_pairs.len() > 3 {
                let topic_name = coordinates
                    .get("topic")
                    .cloned()
                    .unwrap_or_else(|| "General".to_string());
                all_topics
                    .entry(topic_name)
                    .or_default()
                    .push(url.clone());
            }

            for qa in &page_mdoc.qa_pairs {
                all_qa.push(qa.clone());
            }

            if !parsed.paragraphs.is_empty() {
                let first = parsed.paragraphs.first().cloned().unwrap_or_default();
                site_paragaphs.push(first);
            }

            total_tokens += page_mdoc.token_count;
            if !parsed.code_blocks.is_empty() {
                pages_with_code += 1;
            }
            if !parsed.tables.is_empty() {
                pages_with_tables += 1;
            }

            let internal_links: Vec<String> = parsed.links.iter()
                .filter(|l| l.contains(domain))
                .cloned()
                .collect();
            let external_links: Vec<String> = parsed.links.iter()
                .filter(|l| !l.contains(domain))
                .cloned()
                .collect();

            core_pages.push(CorePage {
                url: url.clone(),
                path: path.to_string(),
                title: parsed.title.clone(),
                depth,
                quality_score,
                token_count: page_mdoc.token_count,
                qa_pair_count: page_mdoc.qa_pairs.len(),
                doc: page_doc,
                mdoc: page_mdoc,
                coordinates: page_coords,
                structured_data,
                faq_schema,
                compiled_at: Utc::now().to_rfc3339(),
                links_internal: internal_links,
                links_external: external_links,
            });
        }

        core_pages.sort_by(|a, b| a.path.cmp(&b.path));

        let page_count = core_pages.len();
        let avg_quality = if page_count > 0 {
            core_pages.iter().map(|p| p.quality_score as f64).sum::<f64>() / page_count as f64
        } else {
            0.0
        };

        let best_url = core_pages.iter().max_by_key(|p| p.quality_score).map(|p| p.url.clone()).unwrap_or_default();
        let best_score = core_pages.iter().max_by_key(|p| p.quality_score).map(|p| p.quality_score).unwrap_or(0);
        let worst_url = core_pages.iter().min_by_key(|p| p.quality_score).map(|p| p.url.clone()).unwrap_or_default();
        let worst_score = core_pages.iter().min_by_key(|p| p.quality_score).map(|p| p.quality_score).unwrap_or(0);
        let total_qas = all_qa.len();

        let all_text: String = site_paragaphs.join(" ");
        let site_summary = if all_text.len() > 1000 {
            format!("{}... ({} total chars)", &all_text[..1000], all_text.len())
        } else {
            all_text.clone()
        };

        let site_coordinates = extract_coordinates_from_pages(&core_pages);
        let site_title = core_pages.first().map(|p| p.title.clone()).unwrap_or_default();

        let topics: Vec<CoreTopic> = all_topics
            .into_iter()
            .map(|(name, page_urls)| {
                let confidence = page_urls.len() as f64 / page_count.max(1) as f64;
                CoreTopic {
                    name,
                    confidence: (confidence * 100.0).round() / 100.0,
                    page_urls,
                }
            })
            .collect();

        let site_id_sig = site_id::generate_site_id(domain).unwrap_or_default();
        let content_hash = sha256_hex(&all_text);

        let page_hashes: Vec<CorePageHash> = core_pages
            .iter()
            .map(|p| CorePageHash {
                url: p.url.clone(),
                sha256: sha256_hex(&p.doc.raw_markdown),
            })
            .collect();

        let opt = OptMetadata::new(&domain_obj.name);
        let json_ld = if is_opt { Some(opt.to_json_ld()) } else { None };
        let opt_seo = SeoMetadata {
            title: opt.seo.title.clone(),
            description: opt.seo.description.clone(),
            keywords: opt.seo.keywords.clone(),
            canonical_url: opt.seo.canonical_url.clone(),
            og_title: opt.seo.og_title.clone(),
            og_description: opt.seo.og_description.clone(),
            og_image: opt.seo.og_image.clone(),
            structured_data: opt.seo.structured_data.clone(),
        };
        let opt_geo = GeoMetadata {
            llm_friendly: opt.geo.llm_friendly,
            content_type: opt.geo.content_type.clone(),
            language: opt.geo.language.clone(),
            categories: opt.geo.categories.clone(),
            direct_answers: opt.geo.direct_answers,
            structured_data: opt.geo.structured_data,
        };
        let opt_aeo = AeoMetadata {
            has_qa_pairs: opt.aeo.has_qa_pairs,
            qa_pair_count: opt.aeo.qa_pair_count,
            featured_snippets: opt.aeo.featured_snippets,
            faq_schema: opt.aeo.faq_schema,
            direct_answers: opt.aeo.direct_answers,
            answer_confidence: opt.aeo.answer_confidence,
        };

        let aeo_readiness = if total_qas > 20 {
            95
        } else if total_qas > 10 {
            80
        } else if total_qas > 5 {
            65
        } else if total_qas > 0 {
            40
        } else {
            10
        };

        let overall_score = if page_count > 0 {
            let raw = avg_quality as u32;
            if total_qas > 0 && aeo_readiness > 50 {
                raw.saturating_add(5)
            } else {
                raw
            }
        } else {
            0
        };

        Ok(CoreFile {
            schema: CORE_FILE_SCHEMA.to_string(),
            version: CORE_FILE_VERSION.to_string(),
            compiled_at: Utc::now().to_rfc3339(),
            site: CoreSiteIdentity {
                site_id: site_id.clone(),
                domain: domain.to_string(),
                is_opt,
                title: site_title,
                description: site_summary.chars().take(500).collect(),
                coordinates: site_coordinates,
                total_pages: page_count,
                site_url,
            },
            intelligence: CoreIntelligence {
                site_summary,
                site_token_count: total_tokens,
                all_qa_pairs: all_qa,
                topics,
            },
            pages: core_pages,
            quality: CoreQualityAggregate {
                overall: overall_score,
                avg_page: (avg_quality * 100.0).round() / 100.0,
                best_page: best_url,
                best_score,
                worst_page: worst_url,
                worst_score,
                total_tokens,
                total_qa_pairs: total_qas,
                pages_with_code,
                pages_with_tables,
                aeo_readiness,
            },
            optimization: CoreOptimization {
                seo: opt_seo,
                geo: opt_geo,
                aeo: opt_aeo,
                json_ld,
                faq_schema: None,
            },
            crypto: CoreCrypto {
                site_id_signature: site_id_sig,
                content_root_hash: content_hash,
                page_hashes,
                verified: true,
            },
        })
    }

    pub fn compile_from_directory(
        &self,
        domain: &str,
        dir: &std::path::Path,
    ) -> Result<CoreFile, String> {
        use walkdir::WalkDir;

        let mut pages = Vec::new();

        for entry in WalkDir::new(dir)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if !matches!(ext, "html" | "htm" | "md" | "markdown") {
                continue;
            }

            let content = std::fs::read_to_string(path)
                .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;

            let rel_path = path
                .strip_prefix(dir)
                .unwrap_or(path)
                .to_string_lossy()
                .to_string();

            let url = if domain.ends_with('/') {
                format!("{}{}", domain, rel_path)
            } else {
                format!("{}/{}", domain, rel_path)
            };

            pages.push((url, content));
        }

        self.compile_from_pages(domain, &pages)
    }
}

impl Default for CoreCompiler {
    fn default() -> Self {
        Self::new()
    }
}

fn extract_coordinates_from_pages(pages: &[CorePage]) -> HashMap<String, String> {
    let mut all: HashMap<String, Vec<String>> = HashMap::new();
    for page in pages {
        for (k, v) in &page.coordinates {
            all.entry(k.clone()).or_default().push(v.clone());
        }
    }
    let mut result = HashMap::new();
    for (k, vals) in all {
        let mut counts: HashMap<String, usize> = HashMap::new();
        for v in &vals {
            *counts.entry(v.clone()).or_default() += 1;
        }
        let best = counts.into_iter().max_by_key(|(_, c)| *c).map(|(v, _)| v);
        if let Some(best) = best {
            result.insert(k, best);
        }
    }
    result
}

fn sha256_hex(data: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(data.as_bytes());
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_HTML: &str = r#"
<!DOCTYPE html>
<html>
<head><title>Rust Guide</title></head>
<body>
<h1>Rust Guide</h1>
<p>Rust is a systems programming language focused on safety and performance.</p>
<p>It offers memory safety without garbage collection through its ownership system.</p>
<h2>Getting Started with Rust</h2>
<p>To install Rust, use rustup. It manages Rust versions and associated tools.</p>
<pre><code>curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh</code></pre>
<h2>Axum Web Framework</h2>
<p>Axum is a modular web framework built on tokio, tower, and hyper.</p>
<table>
<tr><th>Feature</th><th>Description</th></tr>
<tr><td>Type-safe routing</td><td>Compile-time path parameter extraction</td></tr>
<tr><td>Middleware</td><td>Tower-based middleware stack</td></tr>
</table>
</body>
</html>
"#;

    #[test]
    fn test_compile_single_page_core() {
        std::env::set_var("RFO_SECRET_KEY", "test-core-secret");
        let compiler = CoreCompiler::new();
        let pages = vec![("https://rust.opt".to_string(), TEST_HTML.to_string())];
        let core = compiler.compile_from_pages("rust.opt", &pages).unwrap();

        assert_eq!(core.schema, "rfo-core-v1");
        assert_eq!(core.site.domain, "rust.opt");
        assert!(core.site.is_opt);
        assert_eq!(core.pages.len(), 1);
        assert!(core.quality.overall > 0);
        assert!(core.quality.total_tokens > 0);
        assert!(!core.crypto.site_id_signature.is_empty());
        assert!(!core.crypto.content_root_hash.is_empty());
    }

    #[test]
    fn test_compile_multi_page_core() {
        std::env::set_var("RFO_SECRET_KEY", "test-core-secret");
        let compiler = CoreCompiler::new();
        let multi_topic_html = r#"
<!DOCTYPE html>
<html><head><title>API Reference</title></head>
<body>
<h1>API Reference</h1>
<p>REST API endpoints for the service.</p>
<h2>Authentication</h2>
<p>Use JWT tokens for API authentication.</p>
<h2>Rate Limiting</h2>
<p>100 requests per minute per user.</p>
<h2>Endpoints</h2>
<p>GET /users, POST /users, GET /posts.</p>
<h2>Error Handling</h2>
<p>Standard HTTP error codes are returned.</p>
<h2>Pagination</h2>
<p>Offset-based pagination with limit parameter.</p>
</body>
</html>
"#;
        let pages = vec![
            ("https://rust.opt/index.html".to_string(), TEST_HTML.to_string()),
            ("https://rust.opt/api.html".to_string(), multi_topic_html.to_string()),
        ];
        let core = compiler.compile_from_pages("rust.opt", &pages).unwrap();

        assert_eq!(core.pages.len(), 2);
        assert_eq!(core.site.total_pages, 2);
        assert!(core.quality.avg_page > 0.0);
        assert!(core.intelligence.topics.len() > 0);
    }

    #[test]
    fn test_core_serialization_roundtrip() {
        std::env::set_var("RFO_SECRET_KEY", "test-core-secret");
        let compiler = CoreCompiler::new();
        let pages = vec![("https://test.opt".to_string(), TEST_HTML.to_string())];
        let core = compiler.compile_from_pages("test.opt", &pages).unwrap();

        let json = serde_json::to_string_pretty(&core).unwrap();
        let deserialized: CoreFile = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.site.domain, core.site.domain);
        assert_eq!(deserialized.pages.len(), core.pages.len());
        assert_eq!(deserialized.pages[0].title, core.pages[0].title);
    }

    #[test]
    fn test_core_page_hashes() {
        std::env::set_var("RFO_SECRET_KEY", "test-core-secret");
        let compiler = CoreCompiler::new();
        let pages = vec![("https://test.opt".to_string(), TEST_HTML.to_string())];
        let core = compiler.compile_from_pages("test.opt", &pages).unwrap();

        assert_eq!(core.crypto.page_hashes.len(), 1);
        assert_eq!(core.crypto.page_hashes[0].sha256.len(), 64);
    }
}
