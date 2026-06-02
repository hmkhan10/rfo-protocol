"""
Low-level FFI bindings to librfo_core via ctypes.

The shared library (librfo_core.so / .dylib / .dll) is loaded
from the standard library search path.
"""

import ctypes
import ctypes.util
import json
import os
from typing import Any, Dict, List, Optional

_lib: Optional[ctypes.CDLL] = None


def _get_lib() -> ctypes.CDLL:
    global _lib
    if _lib is not None:
        return _lib

    lib_name = ctypes.util.find_library("rfo_core")
    if lib_name is None:
        candidates = [
            "librfo_core.so",
            "librfo_core.dylib",
            "rfo_core.dll",
            os.path.join(os.path.dirname(__file__), "..", "target", "release", "librfo_core.so"),
            os.path.join(os.path.dirname(__file__), "..", "target", "release", "librfo_core.dylib"),
            os.path.join(os.path.dirname(__file__), "..", "target", "release", "rfo_core.dll"),
        ]
        for c in candidates:
            if os.path.exists(c):
                lib_name = c
                break

    if lib_name is None:
        raise RuntimeError(
            "librfo_core not found. Build it first:\n"
            "  cd rfo && cargo build --release\n"
            "Then set LD_LIBRARY_PATH=./target/release"
        )

    _lib = ctypes.CDLL(lib_name)
    _lib.rfo_opt_resolver_new.restype = ctypes.c_void_p
    _lib.rfo_opt_resolver_free.argtypes = [ctypes.c_void_p]
    _lib.rfo_opt_register.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_char_p]
    _lib.rfo_opt_register.restype = ctypes.c_int
    _lib.rfo_opt_resolve.argtypes = [ctypes.c_void_p, ctypes.c_char_p]
    _lib.rfo_opt_resolve.restype = ctypes.c_void_p
    _lib.rfo_opt_unregister.argtypes = [ctypes.c_void_p, ctypes.c_char_p]
    _lib.rfo_opt_unregister.restype = ctypes.c_int
    _lib.rfo_opt_contains.argtypes = [ctypes.c_void_p, ctypes.c_char_p]
    _lib.rfo_opt_contains.restype = ctypes.c_int
    _lib.rfo_opt_count.argtypes = [ctypes.c_void_p]
    _lib.rfo_opt_count.restype = ctypes.c_int
    _lib.rfo_free_string.argtypes = [ctypes.c_void_p]
    _lib.rfo_free_string.restype = None
    _lib.rfo_version.restype = ctypes.c_void_p
    _lib.rfo_core_compile.argtypes = [ctypes.c_char_p, ctypes.c_char_p]
    _lib.rfo_core_compile.restype = ctypes.c_void_p
    _lib.rfo_quality_score.argtypes = [ctypes.c_char_p, ctypes.c_char_p]
    _lib.rfo_quality_score.restype = ctypes.c_uint32
    _lib.rfo_compile_file.argtypes = [ctypes.c_char_p, ctypes.c_char_p, ctypes.c_int]
    _lib.rfo_compile_file.restype = ctypes.c_void_p

    return _lib


def _decode_str(ptr) -> Optional[str]:
    if not ptr:
        return None
    try:
        return ctypes.cast(ptr, ctypes.c_char_p).value.decode("utf-8")
    finally:
        _get_lib().rfo_free_string(ptr)


def rfo_version() -> str:
    """Return the version of librfo_core."""
    ptr = _get_lib().rfo_version()
    return _decode_str(ptr) or "unknown"


class OptResolver:
    """Native .opt domain resolver.

    Maps .opt domains (e.g., ``mysite.opt``) to their compiled
    ``.core`` intelligence bundles.  Thread-safe, case-insensitive,
    optionally persistent.
    """

    def __init__(self, owned: bool = True):
        self._lib = _get_lib()
        self._ptr = self._lib.rfo_opt_resolver_new()
        self._owned = owned

    def __del__(self):
        if self._owned and self._ptr:
            self._lib.rfo_opt_resolver_free(self._ptr)

    def register(self, domain: str, core_file: Dict[str, Any]) -> bool:
        """Register a .opt domain with its CoreFile dict."""
        core_json = json.dumps(core_file).encode("utf-8")
        rc = self._lib.rfo_opt_register(self._ptr, domain.encode("utf-8"), core_json)
        return rc == 0

    def resolve(self, domain: str) -> Optional[Dict[str, Any]]:
        """Resolve a .opt domain → CoreFile dict."""
        ptr = self._lib.rfo_opt_resolve(self._ptr, domain.encode("utf-8"))
        raw = _decode_str(ptr)
        if raw is None:
            return None
        return json.loads(raw)

    def unregister(self, domain: str) -> bool:
        """Remove a .opt domain from the registry."""
        rc = self._lib.rfo_opt_unregister(self._ptr, domain.encode("utf-8"))
        return rc == 0

    def contains(self, domain: str) -> bool:
        """Check if a .opt domain is registered."""
        return self._lib.rfo_opt_contains(self._ptr, domain.encode("utf-8")) != 0

    def count(self) -> int:
        """Number of registered .opt domains."""
        return self._lib.rfo_opt_count(self._ptr)


def compile_core(domain: str, directory: str) -> Optional[Dict[str, Any]]:
    """Compile a directory of HTML/MD files into a .core bundle."""
    ptr = _get_lib().rfo_core_compile(domain.encode("utf-8"), directory.encode("utf-8"))
    raw = _decode_str(ptr)
    if raw is None:
        return None
    return json.loads(raw)


def quality_score(mdoc: Dict[str, Any], doc: Dict[str, Any]) -> int:
    """Compute the AI Network Quality Score for a (mdoc, doc) pair."""
    mdoc_json = json.dumps(mdoc).encode("utf-8")
    doc_json = json.dumps(doc).encode("utf-8")
    return _get_lib().rfo_quality_score(mdoc_json, doc_json)
