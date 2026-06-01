use crate::crypto::site_id;
use crate::parser::{generate_qa_pairs, ParsedContent};
use crate::rfo_protocol::{FullDocPayload, MiniDocPayload};

const MDOC_TOKEN_BUDGET: usize = 1500;

/// Estimates token count (rough heuristic: ~4 chars per token).
fn estimate_tokens(text: &str) -> usize {
    text.len() / 4
}

/// Compiles parsed content into a MiniDocPayload (.mdoc).
/// The .mdoc is a token-optimized summary mapped to LLM context windows.
pub fn compile_mdoc(parsed: &ParsedContent) -> MiniDocPayload {
    // Build summary from first 3 paragraphs (sanitized)
    let summary = parsed
        .paragraphs
        .iter()
        .take(3)
        .cloned()
        .collect::<Vec<_>>()
        .join(" ");

    // Generate QaPairs from headings
    let qa_pairs = generate_qa_pairs(parsed);

    // Estimate total token count
    let summary_tokens = estimate_tokens(&summary);
    let qa_tokens: usize = qa_pairs
        .iter()
        .map(|qa| estimate_tokens(&qa.question) + estimate_tokens(&qa.answer))
        .sum();
    let token_count = summary_tokens + qa_tokens;

    MiniDocPayload {
        summary,
        token_count,
        qa_pairs,
    }
}

/// Compiles parsed content into a FullDocPayload (.doc).
/// The .doc is the deep knowledge layout with full markdown and verification.
pub fn compile_doc(parsed: &ParsedContent, domain_url: &str) -> FullDocPayload {
    // Build raw markdown from parsed content
    let mut raw_markdown = String::new();

    if !parsed.title.is_empty() {
        raw_markdown.push_str(&format!("# {}\n\n", parsed.title));
    }

    for heading in &parsed.headings {
        raw_markdown.push_str(&format!("## {}\n\n", heading));
    }

    for para in &parsed.paragraphs {
        raw_markdown.push_str(&format!("{}\n\n", para));
    }

    for code in &parsed.code_blocks {
        raw_markdown.push_str(&format!("```\n{}\n```\n\n", code));
    }

    // Data tables (already markdown-formatted by the parser)
    let data_tables = parsed.tables.clone();

    // Generate verification signature using HMAC-SHA256
    let _content_hash = {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(raw_markdown.as_bytes());
        hex::encode(hasher.finalize())
    };

    let verification_signature = site_id::generate_site_id(domain_url)
        .unwrap_or_else(|_| "unverified".to_string());

    FullDocPayload {
        raw_markdown,
        data_tables,
        verification_signature,
    }
}

/// Calculates the AI Network Quality Score (0-100).
///
/// Scoring factors:
///   - Token density ratio (summary vs full doc) — higher is better
///   - QaPair coverage — more pairs = better AEO readiness
///   - Markdown structural integrity — headings, code blocks, tables
///   - Content length — penalize empty or extremely short content
pub fn calculate_quality_score(mdoc: &MiniDocPayload, doc: &FullDocPayload) -> u8 {
    let mut score: u8 = 0;

    // 1. Token density score (0-30 points)
    //    Good mdoc:token ratio means the summary is concise but informative
    if mdoc.token_count > 0 && mdoc.token_count <= MDOC_TOKEN_BUDGET {
        let ratio = mdoc.summary.len() as f64 / (doc.raw_markdown.len().max(1) as f64);
        let density_score = (ratio * 100.0).min(30.0) as u8;
        score += density_score;
    }

    // 2. QaPair coverage (0-30 points)
    let qa_score = match mdoc.qa_pairs.len() {
        0 => 0,
        1..=3 => 10,
        4..=8 => 20,
        _ => 30,
    };
    score += qa_score;

    // 3. Structural integrity (0-25 points)
    let mut structural = 0u8;
    if !doc.raw_markdown.is_empty() {
        structural += 5;
    }
    if doc.raw_markdown.contains("# ") {
        structural += 5;
    }
    if !doc.data_tables.is_empty() {
        structural += 5;
    }
    if doc.verification_signature != "unverified" && !doc.verification_signature.is_empty() {
        structural += 5;
    }
    if !mdoc.summary.is_empty() {
        structural += 5;
    }
    score += structural.min(25);

    // 4. Content completeness (0-15 points)
    if doc.raw_markdown.len() > 500 {
        score += 5;
    }
    if doc.raw_markdown.len() > 2000 {
        score += 5;
    }
    if !mdoc.summary.is_empty() && mdoc.summary.len() > 50 {
        score += 5;
    }

    score.min(100)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ParsedContent;

    fn setup() {
        std::env::set_var("RFO_SECRET_KEY", "test-compiler-secret-key");
    }

    fn mock_parsed() -> ParsedContent {
        ParsedContent {
            title: "Rust Web Guide".to_string(),
            headings: vec![
                "Getting Started".to_string(),
                "Configuration".to_string(),
                "API Reference".to_string(),
            ],
            paragraphs: vec![
                "Rust is a systems programming language.".to_string(),
                "It offers memory safety without garbage collection.".to_string(),
                "Axum is a popular web framework.".to_string(),
            ],
            code_blocks: vec!["fn main() { println!(\"hello\"); }".to_string()],
            tables: vec!["| Feature | Status |\n| --- | --- |\n| Cache | Ready |".to_string()],
            raw_text: "Rust Web Guide\nRust is a systems programming language.".to_string(),
            links: vec![],
        }
    }

    #[test]
    fn test_compile_mdoc() {
        let parsed = mock_parsed();
        let mdoc = compile_mdoc(&parsed);
        assert!(!mdoc.summary.is_empty());
        assert!(!mdoc.qa_pairs.is_empty());
        assert!(mdoc.token_count > 0);
    }

    #[test]
    fn test_compile_doc() {
        setup();
        let parsed = mock_parsed();
        let doc = compile_doc(&parsed, "https://example.com");
        assert!(doc.raw_markdown.contains("# Rust Web Guide"));
        assert_eq!(doc.data_tables.len(), 1);
        assert!(!doc.verification_signature.is_empty());
    }

    #[test]
    fn test_quality_score() {
        setup();
        let parsed = mock_parsed();
        let mdoc = compile_mdoc(&parsed);
        let doc = compile_doc(&parsed, "https://example.com");
        let score = calculate_quality_score(&mdoc, &doc);
        assert!(score > 30); // Should be reasonably high with this content
        assert!(score <= 100);
    }
}
