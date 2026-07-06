//! NOVA C-ABI FFI seam (ADR-0002). Platform UI shells (Android/Windows) bind to the
//! shared Rust core through these `extern "C"` entry points. Functions that dereference
//! caller-provided raw pointers are `unsafe` and document their pointer contracts.
#![warn(unsafe_op_in_unsafe_fn)]

use nova_ai::AIEngine;
use nova_comms::DeviceComms;
use nova_kernel::{EventMetadata, Kernel, NovaEvent, Result};
use nova_memory::MemoryEngine;
use nova_plugin_host::PluginHost;
use nova_search::UniversalSearch;
use nova_voice::VoiceSystem;
use std::ffi::CStr;
use std::os::raw::c_char;
use std::sync::{Arc, OnceLock};
use tokio::runtime::Runtime;

static TOKIO_RUNTIME: OnceLock<Runtime> = OnceLock::new();

fn get_runtime() -> &'static Runtime {
    TOKIO_RUNTIME
        .get_or_init(|| Runtime::new().expect("Failed to initialize Tokio runtime for FFI"))
}

/// Initialize the NOVA Core and bootstrap all modules.
/// Returns 0 on success, or a negative error code on failure.
///
/// # Safety
/// `config_dir_c` and `log_dir_c` must each be either null or a valid pointer to a
/// NUL-terminated C string that stays valid for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn nova_init(config_dir_c: *const c_char, log_dir_c: *const c_char) -> i32 {
    if config_dir_c.is_null() || log_dir_c.is_null() {
        return -1;
    }

    let config_dir = unsafe {
        match CStr::from_ptr(config_dir_c).to_str() {
            Ok(s) => s,
            Err(_) => return -2,
        }
    };

    let log_dir = unsafe {
        match CStr::from_ptr(log_dir_c).to_str() {
            Ok(s) => s,
            Err(_) => return -3,
        }
    };

    let rt = get_runtime();
    let bootstrap_result: Result<()> = rt.block_on(async {
        let kernel = Kernel::bootstrap(
            std::path::Path::new(config_dir),
            std::path::Path::new(log_dir),
        )?;

        // Register every module with the kernel-managed registry (Milestone 3), then
        // bring them up through the lifecycle manager (initialize -> start) in
        // dependency order. Modules obtain services only through the kernel.
        kernel
            .registry
            .register(Arc::new(MemoryEngine::new(kernel.clone())))?;
        kernel
            .registry
            .register(Arc::new(UniversalSearch::new(kernel.clone())))?;
        kernel
            .registry
            .register(Arc::new(VoiceSystem::new(kernel.clone())))?;
        kernel
            .registry
            .register(Arc::new(AIEngine::new(kernel.clone())))?;
        kernel
            .registry
            .register(Arc::new(DeviceComms::new(kernel.clone())))?;
        kernel
            .registry
            .register(Arc::new(PluginHost::new(kernel.clone())))?;

        kernel.registry.bring_up().await?;

        Ok(())
    });

    match bootstrap_result {
        Ok(_) => 0,
        Err(e) => {
            eprintln!("FFI Boot Error: {}", e);
            -4
        }
    }
}

/// Trigger an event into the NOVA Core.
/// Returns 0 on success.
///
/// # Safety
/// `origin_c`, `action_c`, and `data_json_c` must each be either null or a valid pointer
/// to a NUL-terminated C string that stays valid for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn nova_send_event(
    origin_c: *const c_char,
    action_c: *const c_char,
    data_json_c: *const c_char,
) -> i32 {
    if origin_c.is_null() || action_c.is_null() {
        return -1;
    }

    let origin = unsafe { CStr::from_ptr(origin_c).to_string_lossy().into_owned() };
    let action = unsafe { CStr::from_ptr(action_c).to_string_lossy().into_owned() };
    let _data = if data_json_c.is_null() {
        "".to_string()
    } else {
        unsafe { CStr::from_ptr(data_json_c).to_string_lossy().into_owned() }
    };

    let rt = get_runtime();
    let publish_result: Result<()> = rt.block_on(async {
        let kernel = Kernel::instance()?;

        let metadata = EventMetadata::new(&origin, Some(action));
        let payload: Arc<String> = Arc::new("FFI Event payload".to_string());

        let event = NovaEvent { metadata, payload };

        kernel.event_bus.publish(event)?;
        Ok(())
    });

    match publish_result {
        Ok(_) => 0,
        Err(_) => -2,
    }
}

/// Shutdown the NOVA Core.
/// Returns 0 on success.
#[no_mangle]
pub extern "C" fn nova_shutdown() -> i32 {
    let rt = get_runtime();
    let shutdown_result: Result<()> = rt.block_on(async {
        let kernel = Kernel::instance()?;
        // Tear down modules in reverse dependency order (stop -> shutdown), then the kernel.
        kernel.registry.tear_down().await?;
        kernel.shutdown();
        Ok(())
    });

    match shutdown_result {
        Ok(_) => 0,
        Err(_) => -1,
    }
}
