/**
 * RFO Node.js SDK — Quickstart
 *
 * Prerequisites:
 *   cd rfo && cargo build --release && cd js
 *   npm install
 *   LD_LIBRARY_PATH=../target/release node examples/quickstart.js
 */

const { OptResolver, rfoVersion, compileCore, qualityScore } = require('../index');

function main() {
  console.log(`RFO version: ${rfoVersion()}\n`);

  // ── 1. Resolver ─────────────────────────────────────────────────
  const resolver = new OptResolver();

  const core = {
    schema: 'rfo-core-v1',
    version: '1.0.0',
    compiled_at: '2026-06-02T00:00:00Z',
    site: {
      site_id: 'site_docs_opt',
      domain: 'docs.opt',
      is_opt: true,
      title: 'Docs',
      description: 'Documentation site',
      coordinates: {},
      total_pages: 1,
      site_url: 'https://docs.opt',
    },
    intelligence: {
      site_summary: 'RFO documentation and guides',
      site_token_count: 500,
      all_qa_pairs: [{ question: 'What is RFO?', answer: 'A native AI protocol.' }],
      topics: [{ name: 'Protocol', confidence: 0.9, page_urls: [] }],
    },
    pages: [],
    quality: {
      overall: 92, avg_page: 92.0,
      best_page: '', best_score: 92,
      worst_page: '', worst_score: 0,
      total_tokens: 500, total_qa_pairs: 1,
      pages_with_code: 0, pages_with_tables: 0,
      aeo_readiness: 65,
    },
    optimization: {
      seo: { title: 'Docs', description: 'Documentation', keywords: ['rfo'], canonical_url: 'https://docs.opt/', og_title: null, og_description: null, og_image: null, structured_data: null },
      geo: { llm_friendly: true, content_type: 'documentation', language: 'en', categories: ['tech'], direct_answers: true, structured_data: true },
      aeo: { has_qa_pairs: true, qa_pair_count: 1, featured_snippets: false, faq_schema: false, direct_answers: true, answer_confidence: 85 },
      json_ld: null, faq_schema: null,
    },
    crypto: { site_id_signature: 'sig', content_root_hash: 'hash', page_hashes: [], verified: true },
  };

  resolver.register('docs.opt', core);
  console.log(`Registered docs.opt`);
  console.log(`  Count: ${resolver.count()}`);

  const resolved = resolver.resolve('docs.opt');
  if (resolved) {
    console.log(`  Resolved → ${resolved.site.title}`);
    console.log(`  Quality:  ${resolved.quality.overall}`);
    console.log(`  Verified: ${resolved.crypto.verified}`);
  }

  resolver.destroy();
  console.log('\n✓ Node.js SDK quickstart complete');
}

main();
