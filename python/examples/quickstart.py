"""
RFO Python SDK — Quickstart

Prerequisites:
    cd rfo && cargo build --release && cd python
    pip install -e .
    LD_LIBRARY_PATH=../target/release python examples/quickstart.py
"""

import json
import os
import sys

# Add parent to path so `import rfo` works from examples/
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from rfo import OptResolver, compile_core, quality_score, rfo_version


def main():
    print(f"RFO version: {rfo_version()}")
    print()

    # ── 1. Resolver ─────────────────────────────────────────────────
    resolver = OptResolver()

    # Build a minimal .core file by hand
    core = {
        "schema": "rfo-core-v1",
        "version": "1.0.0",
        "compiled_at": "2026-06-02T00:00:00Z",
        "site": {
            "site_id": "site_docs_opt",
            "domain": "docs.opt",
            "is_opt": True,
            "title": "Docs",
            "description": "Documentation site",
            "coordinates": {},
            "total_pages": 1,
            "site_url": "https://docs.opt",
        },
        "intelligence": {
            "site_summary": "RFO documentation and guides",
            "site_token_count": 500,
            "all_qa_pairs": [
                {"question": "What is RFO?", "answer": "A native AI protocol."}
            ],
            "topics": [{"name": "Protocol", "confidence": 0.9, "page_urls": []}],
        },
        "pages": [],
        "quality": {
            "overall": 92,
            "avg_page": 92.0,
            "best_page": "",
            "best_score": 92,
            "worst_page": "",
            "worst_score": 0,
            "total_tokens": 500,
            "total_qa_pairs": 1,
            "pages_with_code": 0,
            "pages_with_tables": 0,
            "aeo_readiness": 65,
        },
        "optimization": {
            "seo": {
                "title": "Docs",
                "description": "Documentation",
                "keywords": ["rfo", "docs"],
                "canonical_url": "https://docs.opt/",
                "og_title": None,
                "og_description": None,
                "og_image": None,
                "structured_data": None,
            },
            "geo": {
                "llm_friendly": True,
                "content_type": "documentation",
                "language": "en",
                "categories": ["tech"],
                "direct_answers": True,
                "structured_data": True,
            },
            "aeo": {
                "has_qa_pairs": True,
                "qa_pair_count": 1,
                "featured_snippets": False,
                "faq_schema": False,
                "direct_answers": True,
                "answer_confidence": 85,
            },
            "json_ld": None,
            "faq_schema": None,
        },
        "crypto": {
            "site_id_signature": "sig",
            "content_root_hash": "hash",
            "page_hashes": [],
            "verified": True,
        },
    }

    resolver.register("docs.opt", core)
    print(f"Registered docs.opt")
    print(f"  Count: {resolver.count()}")

    resolved = resolver.resolve("docs.opt")
    if resolved:
        print(f"  Resolved → {resolved['site']['title']}")
        print(f"  Quality:  {resolved['quality']['overall']}")
        print(f"  Verified: {resolved['crypto']['verified']}")

    print()

    # ── 2. Compile a directory ─────────────────────────────────────
    demo_dir = os.path.join(os.path.dirname(__file__), "demo_site")
    if os.path.isdir(demo_dir):
        compiled = compile_core("demo.opt", demo_dir)
        if compiled:
            print(f"Compiled demo.opt from {demo_dir}")
            print(f"  Pages: {compiled['site']['total_pages']}")
            print(f"  Quality: {compiled['quality']['overall']}")
    else:
        print(f"Skipping directory compile (no {demo_dir})")

    print()
    print("✓ Python SDK quickstart complete")


if __name__ == "__main__":
    main()
