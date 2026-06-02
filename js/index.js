/**
 * RFO Protocol — Node.js native bindings via Koffi (foreign function interface).
 *
 * Loads librfo_core.so / .dylib / .dll and exposes a high-level API.
 *
 * Usage:
 *   const rfo = require('rfo-core');
 *   const resolver = new rfo.OptResolver();
 *   resolver.register('mysite.opt', coreFileObject);
 *   const core = resolver.resolve('mysite.opt');
 */

const koffi = require('koffi');
const path = require('path');
const fs = require('fs');

// ── Load shared library ──────────────────────────────────────────────

function findLib() {
  const candidates = [
    koffi.resolve('rfo_core'),
    'librfo_core.so',
    'librfo_core.dylib',
    'rfo_core.dll',
    path.join(__dirname, '..', 'target', 'release', 'librfo_core.so'),
    path.join(__dirname, '..', 'target', 'release', 'librfo_core.dylib'),
    path.join(__dirname, '..', 'target', 'release', 'rfo_core.dll'),
  ];
  for (const c of candidates) {
    try {
      if (fs.existsSync(c)) return koffi.load(c);
    } catch { /* try next */ }
  }
  // Let koffi try its own resolution
  return koffi.load('rfo_core');
}

const lib = findLib();

// ── FFI declarations ─────────────────────────────────────────────────

const rfo_opt_resolver_new = lib.func('rfo_opt_resolver_new', 'pointer', []);
const rfo_opt_resolver_free = lib.func('rfo_opt_resolver_free', 'void', ['pointer']);
const rfo_opt_register = lib.func('rfo_opt_register', 'int', ['pointer', 'string', 'string']);
const rfo_opt_resolve = lib.func('rfo_opt_resolve', 'string', ['pointer', 'string']);
const rfo_opt_unregister = lib.func('rfo_opt_unregister', 'int', ['pointer', 'string']);
const rfo_opt_contains = lib.func('rfo_opt_contains', 'int', ['pointer', 'string']);
const rfo_opt_count = lib.func('rfo_opt_count', 'int', ['pointer']);
const rfo_free_string = lib.func('rfo_free_string', 'void', ['pointer']);
const rfo_version_fn = lib.func('rfo_version', 'string', []);
const rfo_core_compile_fn = lib.func('rfo_core_compile', 'string', ['string', 'string']);
const rfo_quality_score_fn = lib.func('rfo_quality_score', 'uint32', ['string', 'string']);

// ── High-level API ───────────────────────────────────────────────────

class OptResolver {
  constructor() {
    this._ptr = rfo_opt_resolver_new();
  }

  destroy() {
    if (this._ptr) {
      rfo_opt_resolver_free(this._ptr);
      this._ptr = null;
    }
  }

  register(domain, coreFile) {
    const json = JSON.stringify(coreFile);
    return rfo_opt_register(this._ptr, domain, json) === 0;
  }

  resolve(domain) {
    const raw = rfo_opt_resolve(this._ptr, domain);
    if (!raw) return null;
    try {
      return JSON.parse(raw);
    } finally {
      rfo_free_string(raw);
    }
  }

  unregister(domain) {
    return rfo_opt_unregister(this._ptr, domain) === 0;
  }

  contains(domain) {
    return rfo_opt_contains(this._ptr, domain) !== 0;
  }

  count() {
    return rfo_opt_count(this._ptr);
  }
}

function rfoVersion() {
  return rfo_version_fn();
}

function compileCore(domain, directory) {
  const raw = rfo_core_compile_fn(domain, directory);
  if (!raw) return null;
  try {
    return JSON.parse(raw);
  } finally {
    rfo_free_string(raw);
  }
}

function qualityScore(mdoc, doc) {
  const mdocJson = JSON.stringify(mdoc);
  const docJson = JSON.stringify(doc);
  return rfo_quality_score_fn(mdocJson, docJson);
}

// ── Exports ──────────────────────────────────────────────────────────

module.exports = {
  OptResolver,
  rfoVersion,
  compileCore,
  qualityScore,
};
