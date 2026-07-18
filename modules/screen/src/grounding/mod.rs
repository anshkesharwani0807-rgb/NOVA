use crate::{GroundingQuery, GroundingResult, UIElementRef, CapturedFrame, Rect, UIElementType};
use async_trait::async_trait;
use std::collections::HashMap;

#[cfg(target_os = "windows")]
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};
#[cfg(target_os = "windows")]
use windows::Win32::UI::Accessibility::*;

pub fn create() -> crate::ScreenResult<std::sync::Arc<dyn crate::VisualGrounding>> {
    #[cfg(target_os = "windows")]
    {
        Ok(std::sync::Arc::new(WindowsVisualGrounding::new()?))
    }
    #[cfg(target_os = "android")]
    {
        Ok(std::sync::Arc::new(AndroidVisualGrounding::new()?))
    }
    #[cfg(not(any(target_os = "windows", target_os = "android")))]
    {
        Err(crate::ScreenError::UnsupportedPlatform)
    }
}

// ---------------------------------------------------------------------------
// Windows – UI Automation (UIA) visual grounding
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
pub struct WindowsVisualGrounding {
    automation: IUIAutomation,
}

#[cfg(target_os = "windows")]
unsafe impl Send for WindowsVisualGrounding {}
#[cfg(target_os = "windows")]
unsafe impl Sync for WindowsVisualGrounding {}

#[cfg(target_os = "windows")]
impl WindowsVisualGrounding {
    pub fn new() -> crate::ScreenResult<Self> {
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
        }
        let automation: IUIAutomation =
            unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)? };
        Ok(Self { automation })
    }

    unsafe fn build_ref(
        &self,
        element: &IUIAutomationElement,
        query: &GroundingQuery,
    ) -> crate::ScreenResult<GroundingResult> {
        let name_raw = element.CurrentName().unwrap_or_default();
        let auto_id_raw = element.CurrentAutomationId().unwrap_or_default();
        let name = name_raw.to_string();
        let auto_id = auto_id_raw.to_string();
        let rect = element.CurrentBoundingRectangle()?;
        let control_type = element.CurrentControlType()?;

        let bounds = Rect {
            x: rect.left,
            y: rect.top,
            width: (rect.right - rect.left).max(0) as u32,
            height: (rect.bottom - rect.top).max(0) as u32,
        };

        let element_type = control_type_to_element_type(control_type);

        let element_id = if !auto_id.is_empty() {
            auto_id.clone()
        } else if !name.is_empty() {
            name.clone()
        } else {
            element
                .GetRuntimeId()
                .ok()
                .map(|id| format!("{:?}", id))
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string())
        };

        let mut attributes = HashMap::new();
        if let Ok(class_name) = element.CurrentClassName() {
            if !class_name.is_empty() {
                attributes.insert("class_name".to_string(), class_name.to_string());
            }
        }
        if let Ok(is_enabled) = element.CurrentIsEnabled() {
            attributes.insert("is_enabled".to_string(), is_enabled.0.to_string());
        }
        if let Ok(framework) = element.CurrentFrameworkId() {
            if !framework.is_empty() {
                attributes.insert("framework".to_string(), framework.to_string());
            }
        }

        let (confidence, match_reason) = score_match(&query.query, &name, &auto_id);

        Ok(GroundingResult {
            element: UIElementRef {
                element_id,
                element_type,
                bounds,
                text: if !name.is_empty() { Some(name) } else { None },
                attributes,
            },
            confidence,
            match_reason,
        })
    }

    unsafe fn walk_tree(
        &self,
        element: &IUIAutomationElement,
        query: &GroundingQuery,
        results: &mut Vec<GroundingResult>,
        limit: usize,
        walker: &IUIAutomationTreeWalker,
    ) -> crate::ScreenResult<()> {
        if results.len() >= limit {
            return Ok(());
        }

        let name_raw = element.CurrentName().unwrap_or_default();
        let auto_id_raw = element.CurrentAutomationId().unwrap_or_default();
        let name = name_raw.to_string();
        let auto_id = auto_id_raw.to_string();

        if matches_query(&query.query, &name) || matches_query(&query.query, &auto_id) {
            if let Ok(result) = self.build_ref(element, query) {
                if result.confidence >= query.confidence_threshold {
                    results.push(result);
                }
            }
        }

        if let Ok(first) = walker.GetFirstChildElement(element) {
            self.walk_tree(&first, query, results, limit, walker)?;
            let mut current = first;
            loop {
                if results.len() >= limit {
                    break;
                }
                match walker.GetNextSiblingElement(&current) {
                    Ok(next) => {
                        self.walk_tree(&next, query, results, limit, walker)?;
                        current = next;
                    }
                    Err(_) => break,
                }
            }
        }

        Ok(())
    }
}

#[cfg(target_os = "windows")]
fn control_type_to_element_type(control_type: UIA_CONTROLTYPE_ID) -> UIElementType {
    #[allow(non_upper_case_globals)]
    match control_type {
        c if c == UIA_ButtonControlTypeId => UIElementType::Button,
        c if c == UIA_CheckBoxControlTypeId => UIElementType::CheckBox,
        c if c == UIA_ComboBoxControlTypeId => UIElementType::ComboBox,
        c if c == UIA_EditControlTypeId => UIElementType::Edit,
        c if c == UIA_HyperlinkControlTypeId => UIElementType::Link,
        c if c == UIA_ImageControlTypeId => UIElementType::Image,
        c if c == UIA_ListControlTypeId => UIElementType::List,
        c if c == UIA_MenuControlTypeId => UIElementType::Menu,
        c if c == UIA_PaneControlTypeId => UIElementType::Pane,
        c if c == UIA_RadioButtonControlTypeId => UIElementType::RadioButton,
        c if c == UIA_ScrollBarControlTypeId => UIElementType::ScrollBar,
        c if c == UIA_SliderControlTypeId => UIElementType::Slider,
        c if c == UIA_StatusBarControlTypeId => UIElementType::StatusBar,
        c if c == UIA_TabControlTypeId => UIElementType::Tab,
        c if c == UIA_TextControlTypeId => UIElementType::TextBlock,
        c if c == UIA_ToolBarControlTypeId => UIElementType::Toolbar,
        c if c == UIA_TreeControlTypeId => UIElementType::Tree,
        c if c == UIA_WindowControlTypeId => UIElementType::Window,
        c if c == UIA_DocumentControlTypeId => UIElementType::Document,
        _ => UIElementType::Custom(format!("{:?}", control_type)),
    }
}

fn matches_query(query: &str, target: &str) -> bool {
    let q = query.trim().to_lowercase();
    let t = target.trim().to_lowercase();
    if q.is_empty() || t.is_empty() {
        return false;
    }
    t.contains(&q) || q.contains(&t)
}

fn score_match(query: &str, name: &str, auto_id: &str) -> (f32, String) {
    let q = query.trim().to_lowercase();
    let n = name.trim().to_lowercase();
    let a = auto_id.trim().to_lowercase();

    if q.is_empty() {
        return (0.0, "Empty query".to_string());
    }

    if !n.is_empty() && n == q {
        return (1.0, format!("Exactly matched element name \"{name}\""));
    }

    if !a.is_empty() && a == q {
        return (0.95, format!("Exactly matched automation ID \"{auto_id}\""));
    }

    if !n.is_empty() && n.starts_with(&q) {
        return (0.9, format!("Element name \"{name}\" starts with query"));
    }

    if !n.is_empty() && n.contains(&q) {
        return (0.8, format!("Element name \"{name}\" contains query"));
    }

    if !a.is_empty() && a.contains(&q) {
        return (0.7, format!("Automation ID \"{auto_id}\" contains query"));
    }

    let query_tokens: Vec<&str> = q.split_whitespace().collect();
    if query_tokens.len() > 1 {
        let matched = query_tokens.iter().filter(|t| n.contains(*t) || a.contains(*t)).count();
        if matched > 0 {
            let ratio = matched as f32 / query_tokens.len() as f32;
            return (0.3 + ratio * 0.3, format!("Matched {matched}/{} query tokens", query_tokens.len()));
        }
    }

    (0.1, format!("Weak match: query \"{query}\" vs name \"{name}\""))
}

#[async_trait]
#[cfg(target_os = "windows")]
impl crate::VisualGrounding for WindowsVisualGrounding {
    fn id(&self) -> &str {
        "windows-uia-grounding"
    }

    async fn locate(&self, _frame: &CapturedFrame, query: &GroundingQuery) -> crate::ScreenResult<GroundingResult> {
        let mut results = Vec::new();
        unsafe {
            let root = self.automation.GetRootElement()?;
            let walker = self.automation.ControlViewWalker()?;
            self.walk_tree(&root, query, &mut results, 1, &walker)?;
        }
        results.into_iter().next().ok_or_else(|| {
            crate::ScreenError::GroundingFailed(format!(
                "No element matching \"{}\" found on screen",
                query.query
            ))
        })
    }

    async fn locate_all(&self, _frame: &CapturedFrame, query: &GroundingQuery) -> crate::ScreenResult<Vec<GroundingResult>> {
        let mut results = Vec::new();
        unsafe {
            let root = self.automation.GetRootElement()?;
            let walker = self.automation.ControlViewWalker()?;
            self.walk_tree(&root, query, &mut results, query.max_results.max(1), &walker)?;
        }
        // Sort descending by confidence
        results.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal));
        Ok(results)
    }
}

// ---------------------------------------------------------------------------
// Android – AccessibilityService-based visual grounding
// ---------------------------------------------------------------------------
//
// Uses the AccessibilityService (set by Kotlin via
// `nativeSetAccessibilityService`) to walk the live UI tree and match
// element text / content-description / view-id against the query string.
// The captured frame is unused (grounding works against the live tree).

#[cfg(target_os = "android")]
use jni::objects::{GlobalRef, JObject, JValue};
#[cfg(target_os = "android")]
use jni::JNIEnv;

#[cfg(target_os = "android")]
fn local_ref<'local>(env: &JNIEnv<'local>, global: &GlobalRef) -> crate::ScreenResult<JObject<'local>> {
    unsafe { env.new_local_ref(global.as_obj()) }
        .map_err(|_| crate::ScreenError::PlatformError("new_local_ref failed".into()))
}

#[cfg(target_os = "android")]
fn obj_to_string(env: &JNIEnv, obj: &JObject) -> String {
    if obj.is_null() {
        return String::new();
    }
    let js = unsafe { jni::objects::JString::from_raw(obj.as_raw()) };
    env.get_string(&js)
        .map(|s| s.into())
        .unwrap_or_default()
}

#[cfg(target_os = "android")]
fn class_name_to_element_type(class_name: &str) -> UIElementType {
    match class_name {
        "android.widget.Button" | "android.widget.ImageButton" => UIElementType::Button,
        "android.widget.EditText" | "android.widget.AutoCompleteTextView" => UIElementType::Edit,
        "android.widget.CheckBox" | "android.widget.Switch" => UIElementType::CheckBox,
        "android.widget.Spinner" | "android.widget.ListView" | "android.widget.GridView" => UIElementType::List,
        "android.widget.ScrollView" | "android.widget.FrameLayout" | "android.widget.LinearLayout"
        | "android.widget.RelativeLayout" | "android.widget.ConstraintLayout" => UIElementType::Pane,
        "android.widget.ImageView" => UIElementType::Image,
        "android.widget.ProgressBar" | "android.widget.SeekBar" => UIElementType::Slider,
        "android.widget.RadioButton" => UIElementType::RadioButton,
        "android.widget.TabHost" | "android.widget.TabWidget" => UIElementType::Tab,
        "android.widget.Toolbar" => UIElementType::Toolbar,
        "android.webkit.WebView" => UIElementType::Document,
        "android.widget.TextView" => UIElementType::TextBlock,
        "android.widget.PopupWindow" | "android.widget.PopupMenu" => UIElementType::Menu,
        s if s.contains("Button") => UIElementType::Button,
        s if s.contains("EditText") => UIElementType::Edit,
        s if s.contains("Text") => UIElementType::TextBlock,
        _ => UIElementType::Custom(class_name.to_string()),
    }
}

#[cfg(target_os = "android")]
pub struct AndroidVisualGrounding {
    java_vm: std::sync::Arc<jni::JavaVM>,
}

#[cfg(target_os = "android")]
impl AndroidVisualGrounding {
    pub fn new() -> crate::ScreenResult<Self> {
        let java_vm = unsafe {
            let vm_ptr = jni::sys::JNI_GetCreatedJavaVMs().map_err(|_| {
                crate::ScreenError::PlatformError(
                    "No Java VM — Android runtime not started".into(),
                )
            })?;
            jni::JavaVM::from_raw(vm_ptr.0 as *mut jni::sys::JavaVM).map_err(|_| {
                crate::ScreenError::PlatformError("Failed to wrap JavaVM handle".into())
            })?
        };

        Ok(Self {
            java_vm: std::sync::Arc::new(java_vm),
        })
    }

    fn get_env(&self) -> crate::ScreenResult<JNIEnv> {
        match self.java_vm.get_env() {
            Ok(env) => Ok(env),
            Err(_) => self
                .java_vm
                .attach_current_thread_as_daemon()
                .map_err(|_| crate::ScreenError::PlatformError("JNI thread attach failed".into())),
        }
    }

    /// Walk the AccessibilityNodeInfo tree and collect matching results.
    fn walk_tree(
        &self,
        env: &JNIEnv,
        node: &JObject,
        query: &GroundingQuery,
        results: &mut Vec<GroundingResult>,
        depth: u32,
    ) {
        const MAX_DEPTH: u32 = 32;
        const MAX_NODES: usize = 200;

        if results.len() >= query.max_results.max(1) || depth > MAX_DEPTH {
            // Recycle this node before returning since we won't recurse
            env.call_method(node, "recycle", "()V", &[]).ok();
            return;
        }

        // --- Read candidate fields -----------------------------------------
        let text = obj_to_string(
            env,
            &env.call_method(node, "getText", "()Ljava/lang/CharSequence;", &[])
                .ok()
                .and_then(|v| v.l().ok())
                .unwrap_or_else(JObject::null),
        );

        let content_desc = obj_to_string(
            env,
            &env.call_method(node, "getContentDescription", "()Ljava/lang/CharSequence;", &[])
                .ok()
                .and_then(|v| v.l().ok())
                .unwrap_or_else(JObject::null),
        );

        let view_id = obj_to_string(
            env,
            &env.call_method(node, "getViewIdResourceName", "()Ljava/lang/String;", &[])
                .ok()
                .and_then(|v| v.l().ok())
                .unwrap_or_else(JObject::null),
        );

        let class_name = obj_to_string(
            env,
            &env.call_method(node, "getClassName", "()Ljava/lang/CharSequence;", &[])
                .ok()
                .and_then(|v| v.l().ok())
                .unwrap_or_else(JObject::null),
        );

        // --- Match against query -------------------------------------------
        let q_lower = query.query.trim().to_lowercase();
        let candidates = [text.as_str(), content_desc.as_str(), view_id.as_str(), class_name.as_str()];
        let mut best_conf: f32 = 0.0;
        let mut best_reason = String::new();
        let mut best_field = String::new();

        for &candidate in &candidates {
            if candidate.is_empty() {
                continue;
            }
            let (conf, reason) = score_match(&q_lower, candidate, "");
            if conf > best_conf {
                best_conf = conf;
                best_reason = reason;
                best_field = candidate.to_string();
            }
        }

        // --- Build bounding rect -------------------------------------------
        let bounds_obj = env
            .call_method(node, "getBoundsInScreen", "()Landroid/graphics/Rect;", &[])
            .ok()
            .and_then(|v| v.l().ok())
            .unwrap_or_else(JObject::null);

        // Wait — getBoundsInScreen takes a Rect param, doesn't return one.
        // Fix: create a Rect, pass it to getBoundsInScreen, then read fields.
        let rect_obj = env
            .new_object("android/graphics/Rect", "()V", &[])
            .unwrap_or_else(|_| JObject::null);
        if !rect_obj.is_null() {
            env.call_method(
                node,
                "getBoundsInScreen",
                "(Landroid/graphics/Rect;)V",
                &[JValue::Object(&rect_obj)],
            )
            .ok();
        }
        let (l, t, r, b) = if !rect_obj.is_null() {
            let left = env.get_field(&rect_obj, "left", "I").ok().and_then(|v| v.i().ok()).unwrap_or(0);
            let top = env.get_field(&rect_obj, "top", "I").ok().and_then(|v| v.i().ok()).unwrap_or(0);
            let right = env.get_field(&rect_obj, "right", "I").ok().and_then(|v| v.i().ok()).unwrap_or(0);
            let bottom = env.get_field(&rect_obj, "bottom", "I").ok().and_then(|v| v.i().ok()).unwrap_or(0);
            (left, top, right, bottom)
        } else {
            (0, 0, 0, 0)
        };

        let bounds = Rect {
            x: l,
            y: t,
            width: (r - l).max(0) as u32,
            height: (b - t).max(0) as u32,
        };

        // --- Collect children before recycling current node -----------------
        let child_count: i32 = env
            .call_method(node, "getChildCount", "()I", &[])
            .ok()
            .and_then(|v| v.i().ok())
            .unwrap_or(0);

        let mut child_jobjects: Vec<JObject> = Vec::new();
        for i in 0..child_count.min(50) {
            if results.len() >= query.max_results.max(1) {
                break;
            }
            if let Ok(child) = env
                .call_method(
                    node,
                    "getChild",
                    "(I)Landroid/view/accessibility/AccessibilityNodeInfo;",
                    &[JValue::Int(i)],
                )
                .and_then(|v| v.l())
            {
                if !child.is_null() {
                    child_jobjects.push(child);
                }
            }
        }

        // Recycle current node — we are done reading its properties.
        env.call_method(node, "recycle", "()V", &[]).ok();

        // --- Emit result if matched ----------------------------------------
        if best_conf > 0.0 && results.len() < query.max_results.max(1) {
            let element_type = class_name_to_element_type(&class_name);
            let element_id = if !view_id.is_empty() {
                view_id.clone()
            } else if !text.is_empty() {
                text.clone()
            } else {
                uuid::Uuid::new_v4().to_string()
            };

            let mut attributes = HashMap::new();
            if !class_name.is_empty() {
                attributes.insert("class_name".to_string(), class_name.clone());
            }
            if !view_id.is_empty() {
                attributes.insert("view_id".to_string(), view_id);
            }
            if !content_desc.is_empty() {
                attributes.insert("content_description".to_string(), content_desc);
            }

            results.push(GroundingResult {
                element: UIElementRef {
                    element_id,
                    element_type,
                    bounds,
                    text: if !text.is_empty() { Some(text) } else { None },
                    attributes,
                },
                confidence: best_conf,
                match_reason: best_reason,
            });
        }

        // --- Recurse into children -----------------------------------------
        if results.len() < query.max_results.max(1) && depth < MAX_DEPTH {
            let max_children = child_jobjects.len().min(MAX_NODES.saturating_sub(results.len()));
            for child in child_jobjects.into_iter().take(max_children) {
                self.walk_tree(env, &child, query, results, depth + 1);
                // child is recycled inside walk_tree (or at the top if depth limit hit)
            }
        } else {
            // Recycle remaining children that weren't visited
            for child in child_jobjects {
                env.call_method(&child, "recycle", "()V", &[]).ok();
            }
        }
    }
}

#[cfg(target_os = "android")]
#[async_trait]
impl crate::VisualGrounding for AndroidVisualGrounding {
    fn id(&self) -> &str {
        "android-accessibility-grounding"
    }

    async fn locate(&self, _frame: &CapturedFrame, query: &GroundingQuery) -> crate::ScreenResult<GroundingResult> {
        let as_ref = crate::ui_tree::get_accessibility_service().ok_or(
            crate::ScreenError::GroundingFailed(
                "AccessibilityService not set — Kotlin must call nativeSetAccessibilityService".into(),
            ),
        )?;

        let env = self.get_env()?;
        let svc = local_ref(&env, as_ref)?;

        let root = env
            .call_method(
                &svc,
                "getRootInActiveWindow",
                "()Landroid/view/accessibility/AccessibilityNodeInfo;",
                &[],
            )?
            .l()?;

        if root.is_null() {
            return Err(crate::ScreenError::GroundingFailed(
                "getRootInActiveWindow returned null — no active window".into(),
            ));
        }

        let mut results = Vec::new();
        let q = GroundingQuery {
            query: query.query.clone(),
            context: query.context.clone(),
            max_results: 1,
            confidence_threshold: query.confidence_threshold,
        };
        self.walk_tree(&env, &root, &q, &mut results, 0);

        results.into_iter().next().ok_or_else(|| {
            crate::ScreenError::GroundingFailed(format!(
                "No element matching \"{}\" found on screen",
                query.query
            ))
        })
    }

    async fn locate_all(&self, _frame: &CapturedFrame, query: &GroundingQuery) -> crate::ScreenResult<Vec<GroundingResult>> {
        let as_ref = crate::ui_tree::get_accessibility_service().ok_or(
            crate::ScreenError::GroundingFailed(
                "AccessibilityService not set — Kotlin must call nativeSetAccessibilityService".into(),
            ),
        )?;

        let env = self.get_env()?;
        let svc = local_ref(&env, as_ref)?;

        let root = env
            .call_method(
                &svc,
                "getRootInActiveWindow",
                "()Landroid/view/accessibility/AccessibilityNodeInfo;",
                &[],
            )?
            .l()?;

        if root.is_null() {
            return Ok(Vec::new());
        }

        let mut results = Vec::new();
        self.walk_tree(&env, &root, query, &mut results, 0);

        results.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal));
        Ok(results)
    }
}
