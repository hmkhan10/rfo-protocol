"""
RFO Protocol — Native Python bindings.

Connects AI agents to .opt domains, .core intelligence bundles,
and the RFO binary protocol without writing Rust.

Usage:
    from rfo import OptResolver

    resolver = OptResolver()
    resolver.register("mysite.opt", core_json)
    core = resolver.resolve("mysite.opt")
    print(core["site"]["title"])
"""

from rfo.ffi import OptResolver, compile_core, quality_score, rfo_version

__all__ = ["OptResolver", "compile_core", "quality_score", "rfo_version"]
__version__ = "0.1.0"
