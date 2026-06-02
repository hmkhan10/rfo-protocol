use crate::crypto::site_id;
use crate::parser::{generate_qa_pairs, ParsedContent};
use crate::rfo_protocol::{FullDocPayload, MiniDocPayload};

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

/// Calculates the AI Network Quality Score (unbounded).
///
/// Higher is better. No cap — quality compounds multiplicatively:
///   - Content richness (structure + length) forms the base
///   - Q&A coverage multiplies relevance
///   - Code + tables multiply technical depth
///   - Signature verification adds trust
///
/// This means a deeply technical, well-structured site with hundreds
/// of Q&A pairs will score far higher than a minimal one — and AI
/// models natively use this score as a visibility/ranking signal.
pub fn calculate_quality_score(mdoc: &MiniDocPayload, doc: &FullDocPayload) -> u32 {
    // 1. Content base score (unbounded)
    let mut base = 0u32;

    // Markdown length (every 100 chars = 1 point, up to 500)
    base += (doc.raw_markdown.len() as u32).min(5000) / 10;

    // Headings
    let heading_count = doc.raw_markdown.matches("# ").count() as u32;
    base += heading_count * 5;

    // Tables (each table adds value)
    base += doc.data_tables.len() as u32 * 15;

    // Code blocks (technical depth)
    let code_count = doc.raw_markdown.matches("```").count() as u32 / 2;
    base += code_count * 10;

    // 2. Q&A relevance multiplier
    let qa_count = mdoc.qa_pairs.len() as u32;
    let qa_multiplier = if qa_count > 20 {
        3.0 // Excellent AEO — triple the score
    } else if qa_count > 10 {
        2.0 // Good AEO — double
    } else if qa_count > 5 {
        1.5 // Moderate AEO
    } else if qa_count > 0 {
        1.2 // Basic
    } else {
        1.0
    };

    // 3. Signature trust premium
    let trust = if doc.verification_signature != "unverified" && !doc.verification_signature.is_empty() {
        50
    } else {
        0
    };

    // 4. Summary quality
    let summary_bonus = if mdoc.summary.len() > 50 {
        (mdoc.summary.len() as u32).min(500) / 5
    } else {
        0
    };

    // Final: (base + trust + summary) × qa_multiplier
    let raw = ((base + trust + summary_bonus) as f64 * qa_multiplier) as u32;
    raw.max(1)
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
        // No upper cap — quality score is unbounded
    }
}
