use std::ffi::{c_char, c_void, CStr, CString};
use std::path::Path;
use std::sync::Arc;

use crate::compiler::{calculate_quality_score, compile_doc, compile_mdoc};
use crate::core_file::{CoreCompiler, CoreFile};
use crate::opt_resolver::{OptResolver, ResolverError};
use crate::parser::{parse_html, parse_markdown};
use crate::rfo_protocol::{FullDocPayload, MiniDocPayload};

fn c_str_to_str<'a>(ptr: *const c_char) -> Result<&'a str, String> {
    if ptr.is_null() {
        return Err("null pointer".to_string());
    }
    unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .map_err(|e| format!("invalid UTF-8: {}", e))
}

fn str_to_c_str(s: &str) -> *mut c_char {
    CString::new(s).unwrap_or_default().into_raw()
}

fn opt_resolver_ptr(resolver: *mut c_void) -> Result<Arc<OptResolver>, String> {
    if resolver.is_null() {
        return Err("null resolver pointer".to_string());
    }
    unsafe {
        let arc: &Arc<OptResolver> = &*(resolver as *const Arc<OptResolver>);
        Ok(arc.clone())
    }
}

// ── Lifetime Management ──────────────────────────────────────────────────

#[no_mangle]
pub extern "C" fn rfo_opt_resolver_new() -> *mut c_void {
    let resolver = Arc::new(OptResolver::new());
    let ptr = Box::into_raw(Box::new(resolver));
    ptr as *mut c_void
}

#[no_mangle]
pub extern "C" fn rfo_opt_resolver_free(ptr: *mut c_void) {
    if ptr.is_null() {
        return;
    }
    unsafe {
        let _ = Box::from_raw(ptr as *mut Arc<OptResolver>);
    }
}

/// Returns a new resolver initialized with any pre-seeded entries from the global registry.
/// The caller owns this pointer and must free it with rfo_opt_resolver_free().
#[no_mangle]
pub extern "C" fn rfo_opt_resolver_default() -> *mut c_void {
    rfo_opt_resolver_new()
}

// ── Registry Operations ──────────────────────────────────────────────────

#[no_mangle]
pub extern "C" fn rfo_opt_register(
    resolver_ptr: *mut c_void,
    domain: *const c_char,
    core_file_json: *const c_char,
) -> i32 {
    let resolver = match opt_resolver_ptr(resolver_ptr) {
        Ok(r) => r,
        Err(_) => return -1,
    };
    let domain = match c_str_to_str(domain) {
        Ok(d) => d,
        Err(_) => return -2,
    };
    let json = match c_str_to_str(core_file_json) {
        Ok(j) => j,
        Err(_) => return -3,
    };
    let core_file: CoreFile = match serde_json::from_str(json) {
        Ok(cf) => cf,
        Err(_) => return -4,
    };
    match resolver.register(domain, core_file) {
        Ok(_) => 0,
        Err(ResolverError::AlreadyRegistered(_)) => 1,
        Err(_) => -5,
    }
}

#[no_mangle]
pub extern "C" fn rfo_opt_register_or_update(
    resolver_ptr: *mut c_void,
    domain: *const c_char,
    core_file_json: *const c_char,
) -> i32 {
    let resolver = match opt_resolver_ptr(resolver_ptr) {
        Ok(r) => r,
        Err(_) => return -1,
    };
    let domain = match c_str_to_str(domain) {
        Ok(d) => d,
        Err(_) => return -2,
    };
    let json = match c_str_to_str(core_file_json) {
        Ok(j) => j,
        Err(_) => return -3,
    };
    let core_file: CoreFile = match serde_json::from_str(json) {
        Ok(cf) => cf,
        Err(_) => return -4,
    };
    match resolver.register_or_update(domain, core_file) {
        Ok(_) => 0,
        Err(_) => -5,
    }
}

#[no_mangle]
pub extern "C" fn rfo_opt_resolve(
    resolver_ptr: *mut c_void,
    domain: *const c_char,
) -> *mut c_char {
    let resolver = match opt_resolver_ptr(resolver_ptr) {
        Ok(r) => r,
        Err(_) => return std::ptr::null_mut(),
    };
    let domain = match c_str_to_str(domain) {
        Ok(d) => d,
        Err(_) => return std::ptr::null_mut(),
    };
    match resolver.resolve(domain) {
        Ok(cf) => {
            match serde_json::to_string(&cf) {
                Ok(json) => str_to_c_str(&json),
                Err(_) => std::ptr::null_mut(),
            }
        }
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub extern "C" fn rfo_opt_unregister(
    resolver_ptr: *mut c_void,
    domain: *const c_char,
) -> i32 {
    let resolver = match opt_resolver_ptr(resolver_ptr) {
        Ok(r) => r,
        Err(_) => return -1,
    };
    let domain = match c_str_to_str(domain) {
        Ok(d) => d,
        Err(_) => return -2,
    };
    match resolver.unregister(domain) {
        Ok(_) => 0,
        Err(_) => -3,
    }
}

#[no_mangle]
pub extern "C" fn rfo_opt_contains(
    resolver_ptr: *mut c_void,
    domain: *const c_char,
) -> i32 {
    let resolver = match opt_resolver_ptr(resolver_ptr) {
        Ok(r) => r,
        Err(_) => return -1,
    };
    let domain = match c_str_to_str(domain) {
        Ok(d) => d,
        Err(_) => return -1,
    };
    if resolver.contains(domain) { 1 } else { 0 }
}

#[no_mangle]
pub extern "C" fn rfo_opt_count(resolver_ptr: *mut c_void) -> i32 {
    let resolver = match opt_resolver_ptr(resolver_ptr) {
        Ok(r) => r,
        Err(_) => return -1,
    };
    resolver.count() as i32
}

// ── .core File Compilation ───────────────────────────────────────────────

#[no_mangle]
pub extern "C" fn rfo_core_compile(
    domain: *const c_char,
    directory: *const c_char,
) -> *mut c_char {
    let domain = match c_str_to_str(domain) {
        Ok(d) => d,
        Err(_) => return std::ptr::null_mut(),
    };
    let dir = match c_str_to_str(directory) {
        Ok(d) => d,
        Err(_) => return std::ptr::null_mut(),
    };
    let compiler = CoreCompiler::new();
    match compiler.compile_from_directory(domain, Path::new(dir)) {
        Ok(core_file) => {
            match serde_json::to_string(&core_file) {
                Ok(json) => str_to_c_str(&json),
                Err(_) => std::ptr::null_mut(),
            }
        }
        Err(_) => std::ptr::null_mut(),
    }
}

// ── Quality Scoring ──────────────────────────────────────────────────────

#[no_mangle]
pub extern "C" fn rfo_quality_score(
    mdoc_json: *const c_char,
    doc_json: *const c_char,
) -> u32 {
    let mdoc_str = match c_str_to_str(mdoc_json) {
        Ok(j) => j,
        Err(_) => return 0,
    };
    let doc_str = match c_str_to_str(doc_json) {
        Ok(j) => j,
        Err(_) => return 0,
    };
    let mdoc: MiniDocPayload = match serde_json::from_str(mdoc_str) {
        Ok(m) => m,
        Err(_) => return 0,
    };
    let doc: FullDocPayload = match serde_json::from_str(doc_str) {
        Ok(d) => d,
        Err(_) => return 0,
    };
    calculate_quality_score(&mdoc, &doc)
}

// ── Single-file Compilation (HTML or Markdown) ──────────────────────────

#[no_mangle]
pub extern "C" fn rfo_compile_file(
    file_path: *const c_char,
    domain: *const c_char,
    is_markdown: i32,
) -> *mut c_char {
    let path = match c_str_to_str(file_path) {
        Ok(p) => p,
        Err(_) => return std::ptr::null_mut(),
    };
    let domain = match c_str_to_str(domain) {
        Ok(d) => d,
        Err(_) => return std::ptr::null_mut(),
    };
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return std::ptr::null_mut(),
    };
    let parsed = if is_markdown != 0 {
        parse_markdown(&content)
    } else {
        parse_html(&content)
    };
    let mdoc = compile_mdoc(&parsed);
    let doc = compile_doc(&parsed, domain);
    let quality = calculate_quality_score(&mdoc, &doc);
    #[derive(serde::Serialize)]
    struct CompileOutput {
        mdoc: MiniDocPayload,
        doc: FullDocPayload,
        quality_score: u32,
    }
    let output = CompileOutput { mdoc, doc, quality_score: quality };
    match serde_json::to_string(&output) {
        Ok(json) => str_to_c_str(&json),
        Err(_) => std::ptr::null_mut(),
    }
}

// ── Memory Management ────────────────────────────────────────────────────

#[no_mangle]
pub extern "C" fn rfo_free_string(s: *mut c_char) {
    if !s.is_null() {
        unsafe {
            let _ = CString::from_raw(s);
        }
    }
}

// ── Version / Capabilities ──────────────────────────────────────────────

#[no_mangle]
pub extern "C" fn rfo_version() -> *mut c_char {
    str_to_c_str(env!("CARGO_PKG_VERSION"))
}

#[no_mangle]
pub extern "C" fn rfo_protocol_version() -> u32 {
    crate::binary::PROTOCOL_VERSION as u32
}
