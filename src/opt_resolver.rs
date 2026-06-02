use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, RwLock};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::core_file::CoreFile;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedOpt {
    pub domain: String,
    pub core_file: CoreFile,
    pub registered_at: DateTime<Utc>,
    pub last_resolved: DateTime<Utc>,
    pub resolve_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptRegistrySnapshot {
    pub entries: HashMap<String, ResolvedOpt>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug)]
pub struct OptResolver {
    registry: Arc<RwLock<HashMap<String, ResolvedOpt>>>,
}

#[derive(Debug)]
pub enum ResolverError {
    NotFound(String),
    AlreadyRegistered(String),
    InvalidDomain(String),
    PersistError(String),
    LoadError(String),
}

impl std::fmt::Display for ResolverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolverError::NotFound(d) => write!(f, ".opt domain not found: {}", d),
            ResolverError::AlreadyRegistered(d) => write!(f, ".opt domain already registered: {}", d),
            ResolverError::InvalidDomain(d) => write!(f, "invalid .opt domain: {}", d),
            ResolverError::PersistError(e) => write!(f, "failed to persist registry: {}", e),
            ResolverError::LoadError(e) => write!(f, "failed to load registry: {}", e),
        }
    }
}

impl std::error::Error for ResolverError {}

impl OptResolver {
    pub fn new() -> Self {
        Self {
            registry: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn with_capacity(cap: usize) -> Self {
        Self {
            registry: Arc::new(RwLock::new(HashMap::with_capacity(cap))),
        }
    }

    pub fn register(&self, domain: &str, core_file: CoreFile) -> Result<(), ResolverError> {
        let domain = domain.trim().to_lowercase();
        if !domain.ends_with(".opt") {
            return Err(ResolverError::InvalidDomain(domain));
        }
        let mut reg = self.registry.write().map_err(|e| {
            ResolverError::PersistError(e.to_string())
        })?;
        if reg.contains_key(&domain) {
            return Err(ResolverError::AlreadyRegistered(domain));
        }
        let now = Utc::now();
        reg.insert(
            domain.clone(),
            ResolvedOpt {
                domain: domain.clone(),
                core_file,
                registered_at: now,
                last_resolved: now,
                resolve_count: 0,
            },
        );
        Ok(())
    }

    pub fn register_or_update(&self, domain: &str, core_file: CoreFile) -> Result<(), ResolverError> {
        let domain = domain.trim().to_lowercase();
        if !domain.ends_with(".opt") {
            return Err(ResolverError::InvalidDomain(domain));
        }
        let mut reg = self.registry.write().map_err(|e| {
            ResolverError::PersistError(e.to_string())
        })?;
        let now = Utc::now();
        if let Some(existing) = reg.get_mut(&domain) {
            existing.core_file = core_file;
            existing.last_resolved = now;
        } else {
            reg.insert(
                domain.clone(),
                ResolvedOpt {
                    domain: domain.clone(),
                    core_file,
                    registered_at: now,
                    last_resolved: now,
                    resolve_count: 0,
                },
            );
        }
        Ok(())
    }

    pub fn resolve(&self, domain: &str) -> Result<CoreFile, ResolverError> {
        let domain = domain.trim().to_lowercase();
        let mut reg = self.registry.write().map_err(|e| {
            ResolverError::PersistError(e.to_string())
        })?;
        let entry = reg.get_mut(&domain).ok_or_else(|| {
            ResolverError::NotFound(domain.clone())
        })?;
        entry.last_resolved = Utc::now();
        entry.resolve_count += 1;
        Ok(entry.core_file.clone())
    }

    pub fn resolve_metadata(&self, domain: &str) -> Result<ResolvedOpt, ResolverError> {
        let domain = domain.trim().to_lowercase();
        let mut reg = self.registry.write().map_err(|e| {
            ResolverError::PersistError(e.to_string())
        })?;
        let entry = reg.get_mut(&domain).ok_or_else(|| {
            ResolverError::NotFound(domain.clone())
        })?;
        entry.last_resolved = Utc::now();
        entry.resolve_count += 1;
        Ok(entry.clone())
    }

    pub fn unregister(&self, domain: &str) -> Result<(), ResolverError> {
        let domain = domain.trim().to_lowercase();
        let mut reg = self.registry.write().map_err(|e| {
            ResolverError::PersistError(e.to_string())
        })?;
        reg.remove(&domain).ok_or_else(|| {
            ResolverError::NotFound(domain)
        })?;
        Ok(())
    }

    pub fn contains(&self, domain: &str) -> bool {
        let domain = domain.trim().to_lowercase();
        let reg = self.registry.read().unwrap();
        reg.contains_key(&domain)
    }

    pub fn list(&self) -> Vec<String> {
        let reg = self.registry.read().unwrap();
        let mut keys: Vec<String> = reg.keys().cloned().collect();
        keys.sort();
        keys
    }

    pub fn count(&self) -> usize {
        let reg = self.registry.read().unwrap();
        reg.len()
    }

    pub fn save_to_file(&self, path: &Path) -> Result<(), ResolverError> {
        let reg = self.registry.read().map_err(|e| {
            ResolverError::PersistError(e.to_string())
        })?;
        let snapshot = OptRegistrySnapshot {
            entries: reg.clone(),
            created_at: Utc::now(),
        };
        let json = serde_json::to_string_pretty(&snapshot)
            .map_err(|e| ResolverError::PersistError(e.to_string()))?;
        std::fs::write(path, json)
            .map_err(|e| ResolverError::PersistError(e.to_string()))?;
        Ok(())
    }

    pub fn load_from_file(path: &Path) -> Result<Self, ResolverError> {
        let json = std::fs::read_to_string(path)
            .map_err(|e| ResolverError::LoadError(e.to_string()))?;
        let snapshot: OptRegistrySnapshot = serde_json::from_str(&json)
            .map_err(|e| ResolverError::LoadError(e.to_string()))?;
        Ok(Self {
            registry: Arc::new(RwLock::new(snapshot.entries)),
        })
    }

    pub fn clone_registry(&self) -> HashMap<String, ResolvedOpt> {
        let reg = self.registry.read().unwrap();
        reg.clone()
    }
}

impl Default for OptResolver {
    fn default() -> Self {
        Self::new()
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
            schema: CORE_FILE_SCHEMA.to_string(),
            version: CORE_FILE_VERSION.to_string(),
            compiled_at: Utc::now().to_rfc3339(),
            site: CoreSiteIdentity {
                site_id: format!("site_{}", domain),
                domain: domain.to_string(),
                is_opt: domain.ends_with(".opt"),
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
                best_page: "https://{}/page1".replace("{}", domain),
                best_score: 90,
                worst_page: "https://{}/page2".replace("{}", domain),
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

    #[test]
    fn test_resolver_new() {
        let resolver = OptResolver::new();
        assert_eq!(resolver.count(), 0);
    }

    #[test]
    fn test_register_and_resolve() {
        let resolver = OptResolver::new();
        let cf = mock_core_file("mysite.opt");
        resolver.register("mysite.opt", cf.clone()).unwrap();
        assert!(resolver.contains("mysite.opt"));
        assert_eq!(resolver.count(), 1);

        let resolved = resolver.resolve("mysite.opt").unwrap();
        assert_eq!(resolved.site.domain, "mysite.opt");
    }

    #[test]
    fn test_register_invalid_domain() {
        let resolver = OptResolver::new();
        let cf = mock_core_file("example.com");
        let err = resolver.register("example.com", cf).unwrap_err();
        assert!(matches!(err, ResolverError::InvalidDomain(_)));
    }

    #[test]
    fn test_register_duplicate() {
        let resolver = OptResolver::new();
        let cf = mock_core_file("dup.opt");
        resolver.register("dup.opt", cf.clone()).unwrap();
        let err = resolver.register("dup.opt", cf).unwrap_err();
        assert!(matches!(err, ResolverError::AlreadyRegistered(_)));
    }

    #[test]
    fn test_register_or_update_replaces() {
        let resolver = OptResolver::new();
        let cf1 = mock_core_file("update.opt");
        resolver.register("update.opt", cf1).unwrap();

        let mut cf2 = mock_core_file("update.opt");
        cf2.site.title = "Updated Title".to_string();
        resolver.register_or_update("update.opt", cf2.clone()).unwrap();

        let resolved = resolver.resolve("update.opt").unwrap();
        assert_eq!(resolved.site.title, "Updated Title");
    }

    #[test]
    fn test_unregister() {
        let resolver = OptResolver::new();
        let cf = mock_core_file("remove.opt");
        resolver.register("remove.opt", cf).unwrap();
        assert_eq!(resolver.count(), 1);
        resolver.unregister("remove.opt").unwrap();
        assert_eq!(resolver.count(), 0);
        assert!(resolver.resolve("remove.opt").is_err());
    }

    #[test]
    fn test_resolve_increments_count() {
        let resolver = OptResolver::new();
        let cf = mock_core_file("stats.opt");
        resolver.register("stats.opt", cf).unwrap();
        let meta = resolver.resolve_metadata("stats.opt").unwrap();
        assert_eq!(meta.resolve_count, 1);
        let meta = resolver.resolve_metadata("stats.opt").unwrap();
        assert_eq!(meta.resolve_count, 2);
    }

    #[test]
    fn test_list_domains() {
        let resolver = OptResolver::new();
        for name in &["alpha.opt", "beta.opt", "gamma.opt"] {
            resolver.register(name, mock_core_file(name)).unwrap();
        }
        let list = resolver.list();
        assert_eq!(list.len(), 3);
        assert_eq!(list, vec!["alpha.opt", "beta.opt", "gamma.opt"]);
    }

    #[test]
    fn test_persist_and_load() {
        let path = Path::new("/tmp/test_opt_registry.json");
        let _ = std::fs::remove_file(path);

        let resolver = OptResolver::new();
        resolver.register("persist.opt", mock_core_file("persist.opt")).unwrap();
        resolver.save_to_file(path).unwrap();

        let loaded = OptResolver::load_from_file(path).unwrap();
        assert_eq!(loaded.count(), 1);
        let resolved = loaded.resolve("persist.opt").unwrap();
        assert_eq!(resolved.site.domain, "persist.opt");

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn test_case_insensitive() {
        let resolver = OptResolver::new();
        let cf = mock_core_file("CaseTest.opt");
        resolver.register("CaseTest.opt", cf).unwrap();
        assert!(resolver.contains("casetest.opt"));
        assert!(resolver.contains("CASETEST.OPT"));
        let resolved = resolver.resolve("CASETEST.OPT").unwrap();
        assert_eq!(resolved.site.domain, "CaseTest.opt");
    }
}
