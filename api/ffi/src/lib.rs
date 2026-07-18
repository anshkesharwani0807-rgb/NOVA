//! NOVA C-ABI FFI seam (ADR-0002). Platform UI shells (Android/Windows) bind to the
//! shared Rust core through these `extern "C"` entry points. Functions that dereference
//! caller-provided raw pointers are `unsafe` and document their pointer contracts.
#![warn(unsafe_op_in_unsafe_fn)]

use nova_ai::AIEngine;
use nova_comms::DeviceComms;
use nova_input::InputSystem;
use nova_kernel::{
    get_config, get_recent_activity, get_recent_egress, update_config, Kernel, NovaConfig, Result,
};
use nova_memory::{MemoryEngine, MemoryRecord, Query};
use nova_plugin_host::PluginHost;
use nova_screen::ScreenSystem;
use nova_search::UniversalSearch;
use nova_voice::VoiceSystem;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::sync::{Arc, OnceLock};
use tokio::runtime::Runtime;

static TOKIO_RUNTIME: OnceLock<Runtime> = OnceLock::new();

static MEMORY: OnceLock<Arc<MemoryEngine>> = OnceLock::new();
static SEARCH: OnceLock<Arc<UniversalSearch>> = OnceLock::new();
static AI: OnceLock<Arc<AIEngine>> = OnceLock::new();
static VOICE: OnceLock<Arc<VoiceSystem>> = OnceLock::new();

fn get_runtime() -> &'static Runtime {
    TOKIO_RUNTIME
        .get_or_init(|| Runtime::new().expect("Failed to initialize Tokio runtime for FFI"))
}

fn ok_json(data: &str) -> *mut c_char {
    CString::new(data)
        .unwrap_or(CString::new("null").unwrap())
        .into_raw()
}

fn error_json(e: impl std::fmt::Display) -> *mut c_char {
    let s = format!(r#"{{"error":"{}"}}"#, e);
    CString::new(s)
        .unwrap_or(CString::new(r#"{"error":"unknown"}"#).unwrap())
        .into_raw()
}

fn cstr<'a>(ptr: *const c_char) -> Result<&'a str> {
    if ptr.is_null() {
        return Err(nova_kernel::NovaError::new(
            nova_kernel::ErrorCategory::ConfigInvalid,
            "ERR_FFI_NULL",
            "null pointer",
        ));
    }
    unsafe { CStr::from_ptr(ptr) }.to_str().map_err(|_| {
        nova_kernel::NovaError::new(
            nova_kernel::ErrorCategory::ConfigInvalid,
            "ERR_FFI_UTF8",
            "invalid UTF-8",
        )
    })
}

fn memory() -> Result<&'static Arc<MemoryEngine>> {
    MEMORY.get().ok_or_else(|| {
        nova_kernel::NovaError::new(
            nova_kernel::ErrorCategory::Kernel,
            "ERR_FFI_NOT_INIT",
            "NOVA not initialized; call nova_init first",
        )
    })
}

fn search() -> Result<&'static Arc<UniversalSearch>> {
    SEARCH.get().ok_or_else(|| {
        nova_kernel::NovaError::new(
            nova_kernel::ErrorCategory::Kernel,
            "ERR_FFI_NOT_INIT",
            "NOVA not initialized; call nova_init first",
        )
    })
}

/// Initialize the NOVA Core and bootstrap all modules.
/// Returns 0 on success, or a negative error code on failure.
///
/// # Safety
/// `config_dir_c` and `log_dir_c` must each be either null or a valid pointer to a
/// NUL-terminated C string that stays valid for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn nova_init(config_dir_c: *const c_char, log_dir_c: *const c_char) -> i32 {
    let config_dir = match cstr(config_dir_c) {
        Ok(s) => s,
        Err(_) => return -1,
    };
    let log_dir = match cstr(log_dir_c) {
        Ok(s) => s,
        Err(_) => return -2,
    };

    let rt = get_runtime();
    let bootstrap_result: Result<()> = rt.block_on(async {
        let kernel = Kernel::bootstrap(
            std::path::Path::new(config_dir),
            std::path::Path::new(log_dir),
        )?;

        let memory = Arc::new(MemoryEngine::new(kernel.clone()));
        let search = Arc::new(UniversalSearch::new(kernel.clone()));
        let voice = Arc::new(VoiceSystem::new(kernel.clone()));
        let ai = Arc::new(AIEngine::new(kernel.clone()));

        MEMORY.set(memory.clone()).map_err(|_| {
            nova_kernel::NovaError::new(
                nova_kernel::ErrorCategory::Kernel,
                "ERR_FFI_ALREADY_INIT",
                "nova_init was called multiple times",
            )
        })?;
        let _ = SEARCH.set(search.clone());
        let _ = VOICE.set(voice.clone());
        let _ = AI.set(ai.clone());

        kernel.registry.register(memory)?;
        kernel.registry.register(search)?;
        kernel.registry.register(voice)?;
        kernel.registry.register(ai)?;
        kernel
            .registry
            .register(Arc::new(DeviceComms::new(kernel.clone())))?;
        kernel
            .registry
            .register(Arc::new(PluginHost::new(kernel.clone())))?;
        let input = Arc::new(InputSystem::new());
        input.set_event_bus(kernel.event_bus.clone());
        kernel.registry.register(input)?;
        kernel
            .registry
            .register(Arc::new(ScreenSystem::new(kernel.clone())))?;

        kernel.registry.bring_up().await?;
        Ok(())
    });

    match bootstrap_result {
        Ok(_) => 0,
        Err(e) => {
            eprintln!("FFI Boot Error: {}", e);
            -3
        }
    }
}

/// Shutdown the NOVA Core. Returns 0 on success.
#[no_mangle]
pub extern "C" fn nova_shutdown() -> i32 {
    let rt = get_runtime();
    let shutdown_result: Result<()> = rt.block_on(async {
        let kernel = Kernel::instance()?;
        kernel.registry.tear_down().await?;
        kernel.shutdown();
        Ok(())
    });

    match shutdown_result {
        Ok(_) => 0,
        Err(_) => -1,
    }
}

// ─── Memory Engine FFI ─────────────────────────────────────────────────────

/// Insert a memory record from JSON. Returns JSON with the record id.
///
/// # Safety
/// `json_c` must be a valid NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn nova_memory_insert(json_c: *const c_char) -> *mut c_char {
    let json = match cstr(json_c) {
        Ok(s) => s,
        Err(_) => return error_json("null pointer"),
    };
    let rec: MemoryRecord = match serde_json::from_str(json) {
        Ok(r) => r,
        Err(e) => return error_json(e),
    };
    let m = match memory() {
        Ok(m) => m,
        Err(e) => return error_json(e),
    };
    if let Err(e) = m.insert(&rec) {
        return error_json(e);
    }
    ok_json(&format!(r#"{{"id":"{}"}}"#, rec.id))
}

/// Search memories with a JSON query. Returns JSON array of records.
///
/// # Safety
/// `query_c` must be a valid NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn nova_memory_search(query_c: *const c_char) -> *mut c_char {
    let q = match cstr(query_c) {
        Ok(s) => s,
        Err(_) => return error_json("null pointer"),
    };
    let query: Query = match serde_json::from_str(q) {
        Ok(q) => q,
        Err(e) => return error_json(e),
    };
    let m = match memory() {
        Ok(m) => m,
        Err(e) => return error_json(e),
    };
    let results = match m.search(&query) {
        Ok(r) => r,
        Err(e) => return error_json(e),
    };
    ok_json(&serde_json::to_string(&results).unwrap_or_else(|_| "[]".to_string()))
}

/// Find a memory by id. Returns JSON record or null.
///
/// # Safety
/// `id_c` must be a valid NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn nova_memory_find_by_id(id_c: *const c_char) -> *mut c_char {
    let id = match cstr(id_c) {
        Ok(s) => s,
        Err(_) => return error_json("null pointer"),
    };
    let m = match memory() {
        Ok(m) => m,
        Err(e) => return error_json(e),
    };
    let result = match m.find_by_id(id) {
        Ok(r) => r,
        Err(e) => return error_json(e),
    };
    match result {
        Some(rec) => ok_json(&serde_json::to_string(&rec).unwrap_or_else(|_| "null".to_string())),
        None => ok_json("null"),
    }
}

/// Soft-delete a memory by id. Returns 0 on success, -1 on error.
///
/// # Safety
/// `id_c` must be a valid NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn nova_memory_delete(id_c: *const c_char) -> i32 {
    let id = match cstr(id_c) {
        Ok(s) => s,
        Err(_) => return -1,
    };
    let m = match memory() {
        Ok(m) => m,
        Err(_) => return -1,
    };
    match m.delete(id) {
        Ok(_) => 0,
        Err(_) => -1,
    }
}

/// Permanently purge a memory by id. Returns 0 on success.
///
/// # Safety
/// `id_c` must be a valid NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn nova_memory_purge(id_c: *const c_char) -> i32 {
    let id = match cstr(id_c) {
        Ok(s) => s,
        Err(_) => return -1,
    };
    let m = match memory() {
        Ok(m) => m,
        Err(_) => return -1,
    };
    match m.purge(id) {
        Ok(_) => 0,
        Err(_) => -1,
    }
}

/// List all memories. Returns JSON array of all records.
#[no_mangle]
pub extern "C" fn nova_memory_list() -> *mut c_char {
    let m = match memory() {
        Ok(m) => m,
        Err(e) => return error_json(e),
    };
    let results = match m.find(&Query::new().include_deleted(true)) {
        Ok(r) => r,
        Err(e) => return error_json(e),
    };
    ok_json(&serde_json::to_string(&results).unwrap_or_else(|_| "[]".to_string()))
}

/// Get total memory count. Returns JSON number.
#[no_mangle]
pub extern "C" fn nova_memory_count() -> *mut c_char {
    let m = match memory() {
        Ok(m) => m,
        Err(e) => return error_json(e),
    };
    let count = match m.total() {
        Ok(c) => c,
        Err(e) => return error_json(e),
    };
    ok_json(&format!("{}", count))
}

// ─── Search Engine FFI ──────────────────────────────────────────────────────

/// Full-text search. Returns JSON array of SearchResult.
///
/// # Safety
/// `text_c` must be a valid NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn nova_search_text(text_c: *const c_char, limit: i32) -> *mut c_char {
    let text = match cstr(text_c) {
        Ok(s) => s,
        Err(_) => return error_json("null pointer"),
    };
    let s = match search() {
        Ok(s) => s,
        Err(e) => return error_json(e),
    };
    let limit = if limit <= 0 {
        None
    } else {
        Some(limit as usize)
    };
    let results = match s.search_text(text, limit) {
        Ok(r) => r,
        Err(e) => return error_json(e),
    };
    ok_json(&serde_json::to_string(&results).unwrap_or_else(|_| "[]".to_string()))
}

/// Natural language search (supports tag:, source:, category: filters).
///
/// # Safety
/// `query_c` must be a valid NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn nova_search_nl(query_c: *const c_char, limit: i32) -> *mut c_char {
    let query = match cstr(query_c) {
        Ok(s) => s,
        Err(_) => return error_json("null pointer"),
    };
    let s = match search() {
        Ok(s) => s,
        Err(e) => return error_json(e),
    };
    let limit = if limit <= 0 {
        None
    } else {
        Some(limit as usize)
    };
    let results = match s.search_nl(query, limit) {
        Ok(r) => r,
        Err(e) => return error_json(e),
    };
    ok_json(&serde_json::to_string(&results).unwrap_or_else(|_| "[]".to_string()))
}

/// Get search index statistics. Returns JSON.
#[no_mangle]
pub extern "C" fn nova_search_stats() -> *mut c_char {
    let s = match search() {
        Ok(s) => s,
        Err(e) => return error_json(e),
    };
    let stats = match s.stats() {
        Ok(st) => st,
        Err(e) => return error_json(e),
    };
    ok_json(&serde_json::to_string(&stats).unwrap_or_else(|_| "{}".to_string()))
}

// ─── Config FFI ─────────────────────────────────────────────────────────────

/// Get the current NOVA config as JSON.
#[no_mangle]
pub extern "C" fn nova_get_config_json() -> *mut c_char {
    let cfg = get_config();
    ok_json(&serde_json::to_string(&cfg).unwrap_or_else(|_| "{}".to_string()))
}

/// Update NOVA config from JSON. Returns 0 on success, -1 on error.
///
/// # Safety
/// `json_c` must be a valid NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn nova_update_config_json(json_c: *const c_char) -> i32 {
    let json = match cstr(json_c) {
        Ok(s) => s,
        Err(_) => return -1,
    };
    let cfg: NovaConfig = match serde_json::from_str(json) {
        Ok(c) => c,
        Err(_) => return -1,
    };
    match update_config(cfg) {
        Ok(_) => 0,
        Err(_) => -1,
    }
}

// ─── Activity Trail & Egress ────────────────────────────────────────────────

/// Get recent activity trail entries as JSON array.
#[no_mangle]
pub extern "C" fn nova_get_activity_trail() -> *mut c_char {
    let entries = get_recent_activity();
    ok_json(&serde_json::to_string(&entries).unwrap_or_else(|_| "[]".to_string()))
}

/// Get recent egress log entries as JSON array.
#[no_mangle]
pub extern "C" fn nova_get_egress_log() -> *mut c_char {
    let entries = get_recent_egress();
    ok_json(&serde_json::to_string(&entries).unwrap_or_else(|_| "[]".to_string()))
}

// ─── Health & Status ────────────────────────────────────────────────────────

/// Get module health report as JSON array.
#[no_mangle]
pub extern "C" fn nova_get_health_report() -> *mut c_char {
    let rt = get_runtime();
    let result: Result<String> = rt.block_on(async {
        let kernel = Kernel::instance()?;
        let report = kernel.registry.list();
        Ok(serde_json::to_string(&report).unwrap_or_else(|_| "[]".to_string()))
    });
    match result {
        Ok(json) => ok_json(&json),
        Err(e) => error_json(e),
    }
}

// ─── Utility ────────────────────────────────────────────────────────────────

/// Free a string allocated by NOVA's FFI. Must be called for every string returned
/// by a `nova_*` function that returns `*mut c_char`.
///
/// # Safety
/// `s` must be a pointer previously returned by a `nova_*` function, or null.
#[no_mangle]
pub unsafe extern "C" fn nova_free_string(s: *mut c_char) {
    if !s.is_null() {
        unsafe {
            let _ = CString::from_raw(s);
        }
    }
}
