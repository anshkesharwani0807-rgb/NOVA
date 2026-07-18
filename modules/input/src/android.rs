use std::sync::Arc;
use std::sync::OnceLock;

use async_trait::async_trait;
use jni::objects::{GlobalRef, JObject, JValue};
use jni::JNIEnv;

use crate::error::{InputError, InputResult};
use crate::traits::InputEngine;
use crate::types::*;

// ---------------------------------------------------------------------------
// Static AccessibilityService reference
// ---------------------------------------------------------------------------

static ACCESSIBILITY_SERVICE: OnceLock<GlobalRef> = OnceLock::new();

pub fn set_accessibility_service(env: &JNIEnv, obj: &JObject) {
    let global = env
        .new_global_ref(obj)
        .expect("android_input: failed to create GlobalRef for AccessibilityService");
    let _ = ACCESSIBILITY_SERVICE.set(global);
}

pub fn get_accessibility_service() -> Option<&'static GlobalRef> {
    ACCESSIBILITY_SERVICE.get()
}

pub fn has_accessibility_service() -> bool {
    ACCESSIBILITY_SERVICE.get().is_some()
}

// ---------------------------------------------------------------------------
// Static JavaVM
// ---------------------------------------------------------------------------

static JAVA_VM: OnceLock<Arc<jni::JavaVM>> = OnceLock::new();

fn get_java_vm() -> InputResult<&'static Arc<jni::JavaVM>> {
    JAVA_VM
        .get_or_try_init(|| unsafe {
            let vm_ptr = jni::sys::JNI_GetCreatedJavaVMs().map_err(|_| {
                InputError::ProviderError("No Java VM — Android runtime not started".to_string())
            })?;
            let vm = jni::JavaVM::from_raw(vm_ptr.0 as *mut jni::sys::JavaVM).map_err(|_| {
                InputError::ProviderError("Failed to wrap JavaVM handle".to_string())
            })?;
            Ok::<Arc<jni::JavaVM>, InputError>(Arc::new(vm))
        })
        .map_err(|e| e.clone())
}

fn get_env() -> InputResult<JNIEnv> {
    let vm = get_java_vm()?;
    match vm.get_env() {
        Ok(env) => Ok(env),
        Err(_) => vm
            .attach_current_thread_as_daemon()
            .map_err(|_| InputError::ProviderError("JNI thread attach failed".to_string())),
    }
}

fn local_ref<'local>(env: &JNIEnv<'local>, global: &GlobalRef) -> InputResult<JObject<'local>> {
    unsafe { env.new_local_ref(global.as_obj()) }
        .map_err(|_| InputError::ProviderError("new_local_ref failed".to_string()))
}

// ---------------------------------------------------------------------------
// Android Input Provider
// ---------------------------------------------------------------------------

pub struct AndroidInputProvider;

impl AndroidInputProvider {
    pub fn new() -> Self {
        // Trigger JavaVM initialization on construction
        let _ = get_java_vm();
        Self
    }

    fn build_tap_gesture(env: &JNIEnv, x: f32, y: f32, duration_ms: i64) -> InputResult<JObject> {
        let path = env
            .new_object("android/graphics/Path", "()V", &[])
            .map_err(|e| InputError::ProviderError(format!("create Path: {e}")))?;
        env.call_method(
            &path,
            "moveTo",
            "(FF)V",
            &[JValue::Float(x), JValue::Float(y)],
        )
        .map_err(|e| InputError::ProviderError(format!("Path.moveTo: {e}")))?;
        env.call_method(
            &path,
            "lineTo",
            "(FF)V",
            &[JValue::Float(x), JValue::Float(y)],
        )
        .map_err(|e| InputError::ProviderError(format!("Path.lineTo: {e}")))?;
        Self::build_gesture(env, &path, 0, duration_ms)
    }

    fn build_swipe_gesture(
        env: &JNIEnv,
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        duration_ms: i64,
    ) -> InputResult<JObject> {
        let path = env
            .new_object("android/graphics/Path", "()V", &[])
            .map_err(|e| InputError::ProviderError(format!("create Path: {e}")))?;
        env.call_method(
            &path,
            "moveTo",
            "(FF)V",
            &[JValue::Float(x1), JValue::Float(y1)],
        )
        .map_err(|e| InputError::ProviderError(format!("Path.moveTo: {e}")))?;
        env.call_method(
            &path,
            "lineTo",
            "(FF)V",
            &[JValue::Float(x2), JValue::Float(y2)],
        )
        .map_err(|e| InputError::ProviderError(format!("Path.lineTo: {e}")))?;
        Self::build_gesture(env, &path, 0, duration_ms)
    }

    fn build_gesture(
        env: &JNIEnv,
        path: &JObject,
        start_time: i64,
        duration_ms: i64,
    ) -> InputResult<JObject> {
        let stroke = env
            .new_object(
                "android/view/accessibility/GestureDescription$StrokeDescription",
                "(Landroid/graphics/Path;JJ)V",
                &[
                    JValue::Object(path),
                    JValue::Long(start_time),
                    JValue::Long(duration_ms),
                ],
            )
            .map_err(|e| InputError::ProviderError(format!("create StrokeDescription: {e}")))?;

        let builder = env
            .new_object(
                "android/view/accessibility/GestureDescription$Builder",
                "()V",
                &[],
            )
            .map_err(|e| InputError::ProviderError(format!("create Builder: {e}")))?;

        env.call_method(
            &builder,
            "addStroke",
            "(Landroid/view/accessibility/GestureDescription$StrokeDescription;)Landroid/view/accessibility/GestureDescription$Builder;",
            &[JValue::Object(&stroke)],
        ).map_err(|e| InputError::ProviderError(format!("addStroke: {e}")))?;

        let gesture = env
            .call_method(
                &builder,
                "build",
                "()Landroid/view/accessibility/GestureDescription;",
                &[],
            )
            .map_err(|e| InputError::ProviderError(format!("build gesture: {e}")))?;

        gesture
            .l()
            .map_err(|e| InputError::ProviderError(format!("gesture.l(): {e}")))
    }

    fn dispatch_gesture(env: &JNIEnv, svc: &JObject, gesture: &JObject) -> InputResult<()> {
        env.call_method(
            svc,
            "dispatchGesture",
            "(Landroid/view/accessibility/GestureDescription;Landroid/view/accessibility/AccessibilityService$GestureResultCallback;Landroid/os/Handler;)V",
            &[JValue::Object(gesture), JValue::Object(&JObject::null()), JValue::Object(&JObject::null())],
        ).map_err(|e| InputError::ProviderError(format!("dispatchGesture: {e}")))?;
        Ok(())
    }

    fn perform_global_action(env: &JNIEnv, svc: &JObject, action_id: i32) -> InputResult<bool> {
        let result = env
            .call_method(
                svc,
                "performGlobalAction",
                "(I)Z",
                &[JValue::Int(action_id)],
            )
            .map_err(|e| InputError::ProviderError(format!("performGlobalAction: {e}")))?;
        Ok(result.z().unwrap_or(false))
    }

    fn type_text_on_focused(env: &JNIEnv, svc: &JObject, text: &str) -> InputResult<bool> {
        let root = env
            .call_method(
                svc,
                "getRootInActiveWindow",
                "()Landroid/view/accessibility/AccessibilityNodeInfo;",
                &[],
            )
            .map_err(|e| InputError::ProviderError(format!("getRootInActiveWindow: {e}")))?
            .l()
            .map_err(|e| InputError::ProviderError(format!("root.l(): {e}")))?;

        if root.is_null() {
            return Err(InputError::ProviderError(
                "getRootInActiveWindow returned null".to_string(),
            ));
        }

        // Find focused node: try FOCUS_INPUT (1), fallback to FOCUS_ACCESSIBILITY (0)
        let mut focused = env
            .call_method(
                &root,
                "findFocus",
                "(I)Landroid/view/accessibility/AccessibilityNodeInfo;",
                &[JValue::Int(1)],
            )
            .and_then(|v| v.l())
            .unwrap_or(JObject::null());

        if focused.is_null() {
            focused = env
                .call_method(
                    &root,
                    "findFocus",
                    "(I)Landroid/view/accessibility/AccessibilityNodeInfo;",
                    &[JValue::Int(0)],
                )
                .and_then(|v| v.l())
                .unwrap_or(JObject::null());
        }

        // Recycle root
        env.call_method(&root, "recycle", "()V", &[]).ok();

        if focused.is_null() {
            return Err(InputError::ProviderError(
                "No focused node found for text input".to_string(),
            ));
        }

        // Ensure focus
        env.call_method(
            &focused,
            "performAction",
            "(I)Z",
            &[JValue::Int(0x00000001)],
        ) // ACTION_FOCUS = 1
        .ok();

        // Build arguments Bundle
        let args = env
            .new_object("android/os/Bundle", "()V", &[])
            .map_err(|e| InputError::ProviderError(format!("create Bundle: {e}")))?;

        let jkey = env
            .new_string("ACTION_ARGUMENT_SET_TEXT_CHARSEQUENCE")
            .map_err(|e| InputError::ProviderError(format!("create key: {e}")))?;
        let jtext = env
            .new_string(text)
            .map_err(|e| InputError::ProviderError(format!("create text: {e}")))?;

        env.call_method(
            &args,
            "putCharSequence",
            "(Ljava/lang/String;Ljava/lang/CharSequence;)V",
            &[JValue::Object(&jkey.into()), JValue::Object(&jtext.into())],
        )
        .map_err(|e| InputError::ProviderError(format!("Bundle.putCharSequence: {e}")))?;

        // ACTION_SET_TEXT = 0x200000
        let performed = env
            .call_method(
                &focused,
                "performAction",
                "(ILandroid/os/Bundle;)Z",
                &[JValue::Int(0x200000), JValue::Object(&args)],
            )
            .map_err(|e| InputError::ProviderError(format!("performAction SET_TEXT: {e}")))?;

        // Recycle focused node
        env.call_method(&focused, "recycle", "()V", &[]).ok();

        Ok(performed.z().unwrap_or(false))
    }
}

impl Default for AndroidInputProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl InputEngine for AndroidInputProvider {
    fn engine_name(&self) -> &'static str {
        "android-accessibility-input"
    }

    async fn execute(&self, action: &InputAction) -> InputResult<ActionResult> {
        let as_ref = get_accessibility_service().ok_or_else(|| {
            InputError::ProviderError(
                "AccessibilityService not set — Kotlin must call nativeSetAccessibilityService"
                    .to_string(),
            )
        })?;

        let env = get_env()?;
        let svc = local_ref(&env, as_ref)?;

        match action {
            // ── Touch actions ────────────────────────────────────────────
            InputAction::Touch(TouchAction::Tap { point }) => {
                let gesture = Self::build_tap_gesture(&env, point.x as f32, point.y as f32, 100)?;
                Self::dispatch_gesture(&env, &svc, &gesture)?;
                Ok(ActionResult::success(format!(
                    "tap at ({}, {})",
                    point.x, point.y
                )))
            }

            InputAction::Touch(TouchAction::DoubleTap { point }) => {
                let g1 = Self::build_tap_gesture(&env, point.x as f32, point.y as f32, 80)?;
                let g2 = Self::build_tap_gesture(&env, point.x as f32, point.y as f32, 80)?;
                Self::dispatch_gesture(&env, &svc, &g1)?;
                Self::dispatch_gesture(&env, &svc, &g2)?;
                Ok(ActionResult::success(format!(
                    "double-tap at ({}, {})",
                    point.x, point.y
                )))
            }

            InputAction::Touch(TouchAction::LongPress { point, duration_ms }) => {
                let dur = (*duration_ms).max(400).min(5000);
                let gesture =
                    Self::build_tap_gesture(&env, point.x as f32, point.y as f32, dur as i64)?;
                Self::dispatch_gesture(&env, &svc, &gesture)?;
                Ok(ActionResult::success(format!(
                    "long-press at ({}, {}) for {}ms",
                    point.x, point.y, dur
                )))
            }

            InputAction::Touch(TouchAction::Swipe {
                from,
                to,
                duration_ms,
            }) => {
                let dur = (*duration_ms).max(50).min(2000);
                let gesture = Self::build_swipe_gesture(
                    &env,
                    from.x as f32,
                    from.y as f32,
                    to.x as f32,
                    to.y as f32,
                    dur as i64,
                )?;
                Self::dispatch_gesture(&env, &svc, &gesture)?;
                Ok(ActionResult::success(format!(
                    "swipe from ({}, {}) to ({}, {})",
                    from.x, from.y, to.x, to.y
                )))
            }

            InputAction::Touch(TouchAction::Pinch {
                center,
                scale,
                duration_ms,
            }) => {
                let dur = (*duration_ms).max(100).min(2000);
                let offset = 200.0f32;

                let path1 = env
                    .new_object("android/graphics/Path", "()V", &[])
                    .map_err(|e| InputError::ProviderError(format!("create Path: {e}")))?;
                let path2 = env
                    .new_object("android/graphics/Path", "()V", &[])
                    .map_err(|e| InputError::ProviderError(format!("create Path: {e}")))?;

                let cx = center.x as f32;
                let cy = center.y as f32;

                if *scale > 1.0 {
                    // Pinch OUT: fingers start at center, move apart
                    env.call_method(
                        &path1,
                        "moveTo",
                        "(FF)V",
                        &[JValue::Float(cx), JValue::Float(cy)],
                    )
                    .ok();
                    env.call_method(
                        &path1,
                        "lineTo",
                        "(FF)V",
                        &[JValue::Float(cx + offset), JValue::Float(cy)],
                    )
                    .ok();
                    env.call_method(
                        &path2,
                        "moveTo",
                        "(FF)V",
                        &[JValue::Float(cx), JValue::Float(cy)],
                    )
                    .ok();
                    env.call_method(
                        &path2,
                        "lineTo",
                        "(FF)V",
                        &[JValue::Float(cx - offset), JValue::Float(cy)],
                    )
                    .ok();
                } else {
                    // Pinch IN: fingers start apart, move to center
                    env.call_method(
                        &path1,
                        "moveTo",
                        "(FF)V",
                        &[JValue::Float(cx + offset), JValue::Float(cy)],
                    )
                    .ok();
                    env.call_method(
                        &path1,
                        "lineTo",
                        "(FF)V",
                        &[JValue::Float(cx), JValue::Float(cy)],
                    )
                    .ok();
                    env.call_method(
                        &path2,
                        "moveTo",
                        "(FF)V",
                        &[JValue::Float(cx - offset), JValue::Float(cy)],
                    )
                    .ok();
                    env.call_method(
                        &path2,
                        "lineTo",
                        "(FF)V",
                        &[JValue::Float(cx), JValue::Float(cy)],
                    )
                    .ok();
                }

                let stroke1 = env
                    .new_object(
                        "android/view/accessibility/GestureDescription$StrokeDescription",
                        "(Landroid/graphics/Path;JJ)V",
                        &[
                            JValue::Object(&path1),
                            JValue::Long(0),
                            JValue::Long(dur as i64),
                        ],
                    )
                    .map_err(|e| {
                        InputError::ProviderError(format!("create StrokeDescription: {e}"))
                    })?;
                let stroke2 = env
                    .new_object(
                        "android/view/accessibility/GestureDescription$StrokeDescription",
                        "(Landroid/graphics/Path;JJ)V",
                        &[
                            JValue::Object(&path2),
                            JValue::Long(0),
                            JValue::Long(dur as i64),
                        ],
                    )
                    .map_err(|e| {
                        InputError::ProviderError(format!("create StrokeDescription: {e}"))
                    })?;

                let builder = env
                    .new_object(
                        "android/view/accessibility/GestureDescription$Builder",
                        "()V",
                        &[],
                    )
                    .map_err(|e| InputError::ProviderError(format!("create Builder: {e}")))?;

                env.call_method(
                    &builder,
                    "addStroke",
                    "(Landroid/view/accessibility/GestureDescription$StrokeDescription;)Landroid/view/accessibility/GestureDescription$Builder;",
                    &[JValue::Object(&stroke1)],
                ).ok();
                env.call_method(
                    &builder,
                    "addStroke",
                    "(Landroid/view/accessibility/GestureDescription$StrokeDescription;)Landroid/view/accessibility/GestureDescription$Builder;",
                    &[JValue::Object(&stroke2)],
                ).ok();

                let gesture = env
                    .call_method(
                        &builder,
                        "build",
                        "()Landroid/view/accessibility/GestureDescription;",
                        &[],
                    )
                    .map_err(|e| InputError::ProviderError(format!("build gesture: {e}")))?
                    .l()
                    .map_err(|e| InputError::ProviderError(format!("gesture.l(): {e}")))?;

                Self::dispatch_gesture(&env, &svc, &gesture)?;

                let action_name = if *scale > 1.0 { "zoom in" } else { "zoom out" };
                Ok(ActionResult::success(format!(
                    "pinch {action_name} at ({}, {}) scale={}",
                    center.x, center.y, scale
                )))
            }

            // ── Mouse → Touch mappings ──────────────────────────────────
            InputAction::Mouse(MouseAction::Click {
                point,
                button,
                count,
            }) => {
                let x = point.x as f32;
                let y = point.y as f32;
                match button {
                    MouseButton::Left => {
                        if *count >= 2 {
                            let g1 = Self::build_tap_gesture(&env, x, y, 80)?;
                            let g2 = Self::build_tap_gesture(&env, x, y, 80)?;
                            Self::dispatch_gesture(&env, &svc, &g1)?;
                            Self::dispatch_gesture(&env, &svc, &g2)?;
                            Ok(ActionResult::success(format!(
                                "double-tap at ({}, {}) via mouse click mapping",
                                point.x, point.y
                            )))
                        } else {
                            let gesture = Self::build_tap_gesture(&env, x, y, 100)?;
                            Self::dispatch_gesture(&env, &svc, &gesture)?;
                            Ok(ActionResult::success(format!(
                                "tap at ({}, {}) via mouse click mapping",
                                point.x, point.y
                            )))
                        }
                    }
                    MouseButton::Right => {
                        // Long press simulates right-click on Android
                        let gesture = Self::build_tap_gesture(&env, x, y, 600)?;
                        Self::dispatch_gesture(&env, &svc, &gesture)?;
                        Ok(ActionResult::success(format!(
                            "long-press at ({}, {}) via right-click mapping",
                            point.x, point.y
                        )))
                    }
                    MouseButton::Middle => Err(InputError::UnsupportedAction(
                        "middle click not supported on Android".to_string(),
                    )),
                }
            }

            InputAction::Mouse(MouseAction::Move { .. }) => Err(InputError::UnsupportedAction(
                "mouse move not supported on Android".to_string(),
            )),

            InputAction::Mouse(MouseAction::Drag {
                from,
                to,
                button: _,
            }) => {
                let gesture = Self::build_swipe_gesture(
                    &env,
                    from.x as f32,
                    from.y as f32,
                    to.x as f32,
                    to.y as f32,
                    300,
                )?;
                Self::dispatch_gesture(&env, &svc, &gesture)?;
                Ok(ActionResult::success(format!(
                    "drag from ({}, {}) to ({}, {})",
                    from.x, from.y, to.x, to.y
                )))
            }

            InputAction::Mouse(MouseAction::Scroll { .. }) => Err(InputError::UnsupportedAction(
                "mouse scroll not supported on Android — use gesture scroll instead".to_string(),
            )),

            // ── Gesture actions ──────────────────────────────────────────
            InputAction::Gesture(GestureAction::Scroll {
                delta_x,
                delta_y,
                smooth: _,
            }) => {
                let (x1, y1, x2, y2) = if *delta_y > 0 {
                    // Scroll up → swipe down
                    (200.0f32, 100.0f32, 200.0f32, 500.0f32)
                } else if *delta_y < 0 {
                    // Scroll down → swipe up
                    (200.0f32, 500.0f32, 200.0f32, 100.0f32)
                } else if *delta_x > 0 {
                    // Scroll right → swipe left
                    (100.0f32, 300.0f32, 500.0f32, 300.0f32)
                } else {
                    // Scroll left → swipe right
                    (500.0f32, 300.0f32, 100.0f32, 300.0f32)
                };
                let gesture = Self::build_swipe_gesture(&env, x1, y1, x2, y2, 200)?;
                Self::dispatch_gesture(&env, &svc, &gesture)?;
                Ok(ActionResult::success(format!(
                    "scroll dx={} dy={}",
                    delta_x, delta_y
                )))
            }

            InputAction::Gesture(GestureAction::Zoom { factor }) => {
                let center_x = 540.0f32;
                let center_y = 960.0f32;
                let offset = 200.0f32;
                let dur: i64 = 300;

                let path1 = env
                    .new_object("android/graphics/Path", "()V", &[])
                    .map_err(|e| InputError::ProviderError(format!("create Path: {e}")))?;
                let path2 = env
                    .new_object("android/graphics/Path", "()V", &[])
                    .map_err(|e| InputError::ProviderError(format!("create Path: {e}")))?;

                if *factor > 1.0 {
                    env.call_method(
                        &path1,
                        "moveTo",
                        "(FF)V",
                        &[JValue::Float(center_x), JValue::Float(center_y)],
                    )
                    .ok();
                    env.call_method(
                        &path1,
                        "lineTo",
                        "(FF)V",
                        &[JValue::Float(center_x + offset), JValue::Float(center_y)],
                    )
                    .ok();
                    env.call_method(
                        &path2,
                        "moveTo",
                        "(FF)V",
                        &[JValue::Float(center_x), JValue::Float(center_y)],
                    )
                    .ok();
                    env.call_method(
                        &path2,
                        "lineTo",
                        "(FF)V",
                        &[JValue::Float(center_x - offset), JValue::Float(center_y)],
                    )
                    .ok();
                } else {
                    env.call_method(
                        &path1,
                        "moveTo",
                        "(FF)V",
                        &[JValue::Float(center_x + offset), JValue::Float(center_y)],
                    )
                    .ok();
                    env.call_method(
                        &path1,
                        "lineTo",
                        "(FF)V",
                        &[JValue::Float(center_x), JValue::Float(center_y)],
                    )
                    .ok();
                    env.call_method(
                        &path2,
                        "moveTo",
                        "(FF)V",
                        &[JValue::Float(center_x - offset), JValue::Float(center_y)],
                    )
                    .ok();
                    env.call_method(
                        &path2,
                        "lineTo",
                        "(FF)V",
                        &[JValue::Float(center_x), JValue::Float(center_y)],
                    )
                    .ok();
                }

                let stroke1 = env
                    .new_object(
                        "android/view/accessibility/GestureDescription$StrokeDescription",
                        "(Landroid/graphics/Path;JJ)V",
                        &[JValue::Object(&path1), JValue::Long(0), JValue::Long(dur)],
                    )
                    .map_err(|e| {
                        InputError::ProviderError(format!("create StrokeDescription: {e}"))
                    })?;
                let stroke2 = env
                    .new_object(
                        "android/view/accessibility/GestureDescription$StrokeDescription",
                        "(Landroid/graphics/Path;JJ)V",
                        &[JValue::Object(&path2), JValue::Long(0), JValue::Long(dur)],
                    )
                    .map_err(|e| {
                        InputError::ProviderError(format!("create StrokeDescription: {e}"))
                    })?;

                let builder = env
                    .new_object(
                        "android/view/accessibility/GestureDescription$Builder",
                        "()V",
                        &[],
                    )
                    .map_err(|e| InputError::ProviderError(format!("create Builder: {e}")))?;

                env.call_method(
                    &builder,
                    "addStroke",
                    "(Landroid/view/accessibility/GestureDescription$StrokeDescription;)Landroid/view/accessibility/GestureDescription$Builder;",
                    &[JValue::Object(&stroke1)],
                ).ok();
                env.call_method(
                    &builder,
                    "addStroke",
                    "(Landroid/view/accessibility/GestureDescription$StrokeDescription;)Landroid/view/accessibility/GestureDescription$Builder;",
                    &[JValue::Object(&stroke2)],
                ).ok();

                let gesture = env
                    .call_method(
                        &builder,
                        "build",
                        "()Landroid/view/accessibility/GestureDescription;",
                        &[],
                    )
                    .map_err(|e| InputError::ProviderError(format!("build gesture: {e}")))?
                    .l()
                    .map_err(|e| InputError::ProviderError(format!("gesture.l(): {e}")))?;

                Self::dispatch_gesture(&env, &svc, &gesture)?;

                let action_name = if *factor > 1.0 { "in" } else { "out" };
                Ok(ActionResult::success(format!(
                    "zoom {action_name} factor={factor}"
                )))
            }

            InputAction::Gesture(GestureAction::ThreeFingerSwipe { direction }) => {
                Err(InputError::UnsupportedAction(format!(
                    "three-finger swipe {:?} not supported via GestureDescription",
                    direction
                )))
            }

            // ── Keyboard actions ─────────────────────────────────────────
            InputAction::Keyboard(KeyboardAction::TypeText { text }) => {
                let ok = Self::type_text_on_focused(&env, &svc, text)?;
                if ok {
                    Ok(ActionResult::success(format!(
                        "typed '{}'",
                        truncate(text, 40)
                    )))
                } else {
                    Err(InputError::ProviderError(
                        "text input action returned false".to_string(),
                    ))
                }
            }

            InputAction::Keyboard(KeyboardAction::KeyPress { key, modifiers: _ }) => {
                match key.to_lowercase().as_str() {
                    "back" => {
                        let ok = Self::perform_global_action(&env, &svc, 1)?;
                        if ok {
                            Ok(ActionResult::success("back".to_string()))
                        } else {
                            Err(InputError::ProviderError("back action failed".to_string()))
                        }
                    }
                    "home" => {
                        let ok = Self::perform_global_action(&env, &svc, 2)?;
                        if ok {
                            Ok(ActionResult::success("home".to_string()))
                        } else {
                            Err(InputError::ProviderError("home action failed".to_string()))
                        }
                    }
                    "recents" | "overview" => {
                        let ok = Self::perform_global_action(&env, &svc, 3)?;
                        if ok {
                            Ok(ActionResult::success("recents".to_string()))
                        } else {
                            Err(InputError::ProviderError(
                                "recents action failed".to_string(),
                            ))
                        }
                    }
                    "notifications" => {
                        let ok = Self::perform_global_action(&env, &svc, 4)?;
                        if ok {
                            Ok(ActionResult::success("notifications".to_string()))
                        } else {
                            Err(InputError::ProviderError(
                                "notifications action failed".to_string(),
                            ))
                        }
                    }
                    "screenshot" => {
                        let ok = Self::perform_global_action(&env, &svc, 9)?;
                        if ok {
                            Ok(ActionResult::success("screenshot".to_string()))
                        } else {
                            Err(InputError::ProviderError(
                                "screenshot action failed".to_string(),
                            ))
                        }
                    }
                    _ => Err(InputError::UnsupportedAction(format!(
                        "key not supported on Android: {key}"
                    ))),
                }
            }

            InputAction::Keyboard(KeyboardAction::KeyRelease { key }) => {
                Err(InputError::UnsupportedAction(format!(
                    "key release not supported on Android: {key}"
                )))
            }

            InputAction::Keyboard(KeyboardAction::Hotkey { keys }) => {
                Err(InputError::UnsupportedAction(format!(
                    "hotkeys not supported on Android: {:?}",
                    keys
                )))
            }

            // ── Other actions ────────────────────────────────────────────
            InputAction::Wait { duration_ms } => {
                tokio::time::sleep(std::time::Duration::from_millis(*duration_ms)).await;
                Ok(ActionResult::success(format!("waited {}ms", duration_ms)))
            }
        }
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max])
    }
}
