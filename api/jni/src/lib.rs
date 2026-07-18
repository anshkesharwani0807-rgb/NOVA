//! NOVA JNI bridge — Android (Kotlin/Java) bindings over the C-ABI FFI.
//! The Kotlin side declares a `NovaCore` object with `external fun` declarations
//! matching these JNI entry points. All string results are JSON.
#![warn(unsafe_op_in_unsafe_fn)]

use jni::objects::{JClass, JString};
use jni::sys::{jboolean, jint, jstring, JNI_TRUE};
use jni::JNIEnv;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;

fn to_cstring(env: &mut JNIEnv, input: &JString) -> Result<CString, ()> {
    let java_str: String = env.get_string(input).map_err(|_| ())?.into();
    CString::new(java_str).map_err(|_| ())
}

fn jni_result(env: &mut JNIEnv, output: *mut c_char) -> jstring {
    if output.is_null() {
        return std::ptr::null_mut();
    }
    let c_str = unsafe { CStr::from_ptr(output) };
    let rust_str = c_str.to_string_lossy();
    let result = env.new_string(&*rust_str);
    unsafe { nova_ffi::nova_free_string(output) };
    match result {
        Ok(s) => s.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub extern "system" fn Java_com_example_nova_NovaCore_nativeInit(
    mut env: JNIEnv,
    _class: JClass,
    config_dir: JString,
    log_dir: JString,
) {
    let cfg = match to_cstring(&mut env, &config_dir) {
        Ok(s) => s,
        Err(_) => return,
    };
    let log = match to_cstring(&mut env, &log_dir) {
        Ok(s) => s,
        Err(_) => return,
    };
    unsafe {
        nova_ffi::nova_init(cfg.as_ptr(), log.as_ptr());
    }
}

#[no_mangle]
pub extern "system" fn Java_com_example_nova_NovaCore_nativeShutdown(_env: JNIEnv, _class: JClass) {
    nova_ffi::nova_shutdown();
}

#[no_mangle]
pub extern "system" fn Java_com_example_nova_NovaCore_nativeMemoryInsert(
    mut env: JNIEnv,
    _class: JClass,
    json: JString,
) -> jstring {
    let json_c = match to_cstring(&mut env, &json) {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let result = unsafe { nova_ffi::nova_memory_insert(json_c.as_ptr()) };
    jni_result(&mut env, result)
}

#[no_mangle]
pub extern "system" fn Java_com_example_nova_NovaCore_nativeMemorySearch(
    mut env: JNIEnv,
    _class: JClass,
    query: JString,
) -> jstring {
    let query_c = match to_cstring(&mut env, &query) {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let result = unsafe { nova_ffi::nova_memory_search(query_c.as_ptr()) };
    jni_result(&mut env, result)
}

#[no_mangle]
pub extern "system" fn Java_com_example_nova_NovaCore_nativeMemoryFindById(
    mut env: JNIEnv,
    _class: JClass,
    id: JString,
) -> jstring {
    let id_c = match to_cstring(&mut env, &id) {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let result = unsafe { nova_ffi::nova_memory_find_by_id(id_c.as_ptr()) };
    jni_result(&mut env, result)
}

#[no_mangle]
pub extern "system" fn Java_com_example_nova_NovaCore_nativeMemoryDelete(
    mut env: JNIEnv,
    _class: JClass,
    id: JString,
) -> jboolean {
    let id_c = match to_cstring(&mut env, &id) {
        Ok(s) => s,
        Err(_) => return 0,
    };
    let result = unsafe { nova_ffi::nova_memory_delete(id_c.as_ptr()) };
    if result == 0 {
        JNI_TRUE
    } else {
        0
    }
}

#[no_mangle]
pub extern "system" fn Java_com_example_nova_NovaCore_nativeMemoryList(
    mut env: JNIEnv,
    _class: JClass,
) -> jstring {
    let result = nova_ffi::nova_memory_list();
    jni_result(&mut env, result)
}

#[no_mangle]
pub extern "system" fn Java_com_example_nova_NovaCore_nativeSearchText(
    mut env: JNIEnv,
    _class: JClass,
    text: JString,
    limit: jint,
) -> jstring {
    let text_c = match to_cstring(&mut env, &text) {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let result = unsafe { nova_ffi::nova_search_text(text_c.as_ptr(), limit) };
    jni_result(&mut env, result)
}

#[no_mangle]
pub extern "system" fn Java_com_example_nova_NovaCore_nativeSearchNl(
    mut env: JNIEnv,
    _class: JClass,
    query: JString,
    limit: jint,
) -> jstring {
    let query_c = match to_cstring(&mut env, &query) {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let result = unsafe { nova_ffi::nova_search_nl(query_c.as_ptr(), limit) };
    jni_result(&mut env, result)
}

#[no_mangle]
pub extern "system" fn Java_com_example_nova_NovaCore_nativeGetActivityTrail(
    mut env: JNIEnv,
    _class: JClass,
) -> jstring {
    let result = nova_ffi::nova_get_activity_trail();
    jni_result(&mut env, result)
}

#[no_mangle]
pub extern "system" fn Java_com_example_nova_NovaCore_nativeGetEgressLog(
    mut env: JNIEnv,
    _class: JClass,
) -> jstring {
    let result = nova_ffi::nova_get_egress_log();
    jni_result(&mut env, result)
}

#[no_mangle]
pub extern "system" fn Java_com_example_nova_NovaCore_nativeGetConfig(
    mut env: JNIEnv,
    _class: JClass,
) -> jstring {
    let result = nova_ffi::nova_get_config_json();
    jni_result(&mut env, result)
}

#[no_mangle]
pub extern "system" fn Java_com_example_nova_NovaCore_nativeUpdateConfig(
    mut env: JNIEnv,
    _class: JClass,
    json: JString,
) -> jboolean {
    let json_c = match to_cstring(&mut env, &json) {
        Ok(s) => s,
        Err(_) => return 0,
    };
    let result = unsafe { nova_ffi::nova_update_config_json(json_c.as_ptr()) };
    if result == 0 {
        JNI_TRUE
    } else {
        0
    }
}

#[no_mangle]
pub extern "system" fn Java_com_example_nova_NovaCore_nativeGetHealthReport(
    mut env: JNIEnv,
    _class: JClass,
) -> jstring {
    let result = nova_ffi::nova_get_health_report();
    jni_result(&mut env, result)
}

#[no_mangle]
pub extern "system" fn Java_com_example_nova_NovaCore_nativeMemoryCount(
    mut env: JNIEnv,
    _class: JClass,
) -> jstring {
    let result = nova_ffi::nova_memory_count();
    jni_result(&mut env, result)
}

#[no_mangle]
pub extern "system" fn Java_com_example_nova_NovaCore_nativeSearchStats(
    mut env: JNIEnv,
    _class: JClass,
) -> jstring {
    let result = nova_ffi::nova_search_stats();
    jni_result(&mut env, result)
}

// ---------------------------------------------------------------------------
// Android screen capture JNI entry points (Android-only)
// ---------------------------------------------------------------------------

/// Kotlin calls this once at app start to provide the Application Context.
/// Required by AndroidScreenCapture for DisplayMetrics queries.
#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_com_example_nova_NovaCore_nativeSetApplicationContext(
    mut env: JNIEnv,
    _class: JClass,
    context: JObject,
) {
    nova_screen::jni_bridge::set_application_context(&env, &context);
}

/// Kotlin calls this after the user grants screen-capture permission
/// via `MediaProjectionManager.getMediaProjection(resultCode, data)`.
#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_com_example_nova_NovaCore_nativeSetMediaProjection(
    mut env: JNIEnv,
    _class: JClass,
    media_projection: JObject,
) {
    nova_screen::capture::android::set_media_projection(&env, &media_projection);
}

/// Returns JNI_TRUE if a MediaProjection reference is available.
#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_com_example_nova_NovaCore_nativeHasMediaProjection(
    _env: JNIEnv,
    _class: JClass,
) -> jboolean {
    if nova_screen::capture::android::has_media_projection() {
        JNI_TRUE
    } else {
        0
    }
}

/// Kotlin calls this when the AccessibilityService connects
/// (`onServiceConnected`).  `service` is the
/// `android.accessibilityservice.AccessibilityService` instance.
/// Sets the service reference for both screen (perception) and input (control).
#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_com_example_nova_NovaCore_nativeSetAccessibilityService(
    mut env: JNIEnv,
    _class: JClass,
    service: JObject,
) {
    nova_screen::ui_tree::set_accessibility_service(&env, &service);
    nova_input::android_set_accessibility_service(&env, &service);
}

/// Returns JNI_TRUE if the AccessibilityService reference is available.
#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_com_example_nova_NovaCore_nativeHasAccessibilityService(
    _env: JNIEnv,
    _class: JClass,
) -> jboolean {
    if nova_screen::ui_tree::has_accessibility_service() {
        JNI_TRUE
    } else {
        0
    }
}
