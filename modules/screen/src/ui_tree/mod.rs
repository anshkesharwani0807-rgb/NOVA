//! UI Tree extraction for Windows (UI Automation) and Android (AccessibilityService)

use crate::Rect;
use async_trait::async_trait;
use std::collections::HashMap;
#[cfg(target_os = "windows")]
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};
#[cfg(target_os = "windows")]
use windows::Win32::UI::Accessibility::*;

#[cfg(target_os = "windows")]
pub struct WindowsUITreeExtractor {
    automation: IUIAutomation,
}

#[cfg(target_os = "windows")]
unsafe impl Send for WindowsUITreeExtractor {}
#[cfg(target_os = "windows")]
unsafe impl Sync for WindowsUITreeExtractor {}

#[cfg(target_os = "windows")]
impl WindowsUITreeExtractor {
    pub fn new() -> crate::ScreenResult<Self> {
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
        }
        let automation: IUIAutomation =
            unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)? };
        Ok(Self { automation })
    }

    fn control_type_to_element_type(control_type: UIA_CONTROLTYPE_ID) -> crate::UIElementType {
        #[allow(non_upper_case_globals)]
        match control_type {
            c if c == UIA_ButtonControlTypeId => crate::UIElementType::Button,
            c if c == UIA_CheckBoxControlTypeId => crate::UIElementType::CheckBox,
            c if c == UIA_ComboBoxControlTypeId => crate::UIElementType::ComboBox,
            c if c == UIA_EditControlTypeId => crate::UIElementType::Edit,
            c if c == UIA_HyperlinkControlTypeId => crate::UIElementType::Link,
            c if c == UIA_ImageControlTypeId => crate::UIElementType::Image,
            c if c == UIA_ListControlTypeId => crate::UIElementType::List,
            c if c == UIA_MenuControlTypeId => crate::UIElementType::Menu,
            c if c == UIA_PaneControlTypeId => crate::UIElementType::Pane,
            c if c == UIA_RadioButtonControlTypeId => crate::UIElementType::RadioButton,
            c if c == UIA_ScrollBarControlTypeId => crate::UIElementType::ScrollBar,
            c if c == UIA_SliderControlTypeId => crate::UIElementType::Slider,
            c if c == UIA_StatusBarControlTypeId => crate::UIElementType::StatusBar,
            c if c == UIA_TabControlTypeId => crate::UIElementType::Tab,
            c if c == UIA_TextControlTypeId => crate::UIElementType::TextBlock,
            c if c == UIA_ToolBarControlTypeId => crate::UIElementType::Toolbar,
            c if c == UIA_TreeControlTypeId => crate::UIElementType::Tree,
            c if c == UIA_WindowControlTypeId => crate::UIElementType::Window,
            c if c == UIA_DocumentControlTypeId => crate::UIElementType::Document,
            _ => crate::UIElementType::Custom(format!("{:?}", control_type)),
        }
    }

    unsafe fn build_element(
        &self,
        element: &IUIAutomationElement,
    ) -> crate::ScreenResult<crate::UIElement> {
        let name = element.CurrentName().unwrap_or_default();
        let name_str = if !name.is_empty() {
            Some(name.to_string())
        } else {
            None
        };

        let auto_id = element.CurrentAutomationId().unwrap_or_default();
        let auto_id_str = if !auto_id.is_empty() {
            Some(auto_id.to_string())
        } else {
            None
        };

        let class_name = element.CurrentClassName().unwrap_or_default();
        let class_name_str = if !class_name.is_empty() {
            Some(class_name.to_string())
        } else {
            None
        };

        let rect = element.CurrentBoundingRectangle()?;
        let bounds = Rect {
            x: rect.left,
            y: rect.top,
            width: (rect.right - rect.left).max(0) as u32,
            height: (rect.bottom - rect.top).max(0) as u32,
        };

        let control_type = element.CurrentControlType()?;
        let element_type = Self::control_type_to_element_type(control_type);

        let framework_id = element.CurrentFrameworkId().unwrap_or_default();
        let mut attributes = HashMap::new();
        if !framework_id.is_empty() {
            attributes.insert("framework_id".to_string(), framework_id.to_string());
        }
        if let Ok(is_enabled) = element.CurrentIsEnabled() {
            attributes.insert("is_enabled".to_string(), is_enabled.0.to_string());
        }
        if let Ok(help_text) = element.CurrentHelpText() {
            if !help_text.is_empty() {
                attributes.insert("help_text".to_string(), help_text.to_string());
            }
        }

        let element_id = auto_id_str.clone().unwrap_or_else(|| {
            element
                .GetRuntimeId()
                .ok()
                .map(|id| format!("{:?}", id))
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string())
        });

        let children = self.build_children(element)?;

        Ok(crate::UIElement {
            element_id,
            element_type,
            bounds,
            name: name_str,
            text: None,
            automation_id: auto_id_str,
            class_name: class_name_str,
            children,
            attributes,
        })
    }

    unsafe fn build_children(
        &self,
        parent: &IUIAutomationElement,
    ) -> crate::ScreenResult<Vec<crate::UIElement>> {
        let walker = self.automation.ControlViewWalker()?;
        let mut children = Vec::new();

        if let Ok(first) = walker.GetFirstChildElement(parent) {
            children.push(self.build_element(&first)?);
            let mut current = first;
            while let Ok(next) = walker.GetNextSiblingElement(&current) {
                children.push(self.build_element(&next)?);
                current = next;
            }
        }

        Ok(children)
    }
}

#[async_trait]
#[cfg(target_os = "windows")]
impl crate::UITreeExtractor for WindowsUITreeExtractor {
    fn id(&self) -> &str {
        "windows-uia"
    }

    async fn extract_tree(&self, _frame: &crate::CapturedFrame) -> crate::ScreenResult<crate::UITree> {
        let root = unsafe { self.automation.GetRootElement()? };
        let element = unsafe { self.build_element(&root)? };
        Ok(crate::UITree {
            root: element,
            timestamp: chrono::Utc::now().timestamp_millis(),
        })
    }

    async fn find_element(
        &self,
        tree: &crate::UITree,
        query: &crate::GroundingQuery,
    ) -> crate::ScreenResult<Option<crate::UIElementRef>> {
        fn find_recursive(
            element: &crate::UIElement,
            query: &crate::GroundingQuery,
        ) -> Option<crate::UIElementRef> {
            let name = element.name.as_deref().unwrap_or("");
            let text = element.text.as_deref().unwrap_or("");
            let combined = format!("{} {}", name, text)
                .to_lowercase();
            let q = query.query.to_lowercase();
            if combined.contains(&q) {
                return Some(crate::UIElementRef {
                    element_id: element.element_id.clone(),
                    element_type: element.element_type.clone(),
                    bounds: element.bounds,
                    text: element.text.clone(),
                    attributes: element.attributes.clone(),
                });
            }
            for child in &element.children {
                if let Some(found) = find_recursive(child, query) {
                    return Some(found);
                }
            }
            None
        }
        Ok(find_recursive(&tree.root, query))
    }

    async fn get_element_bounds(
        &self,
        element: &crate::UIElementRef,
    ) -> crate::ScreenResult<Rect> {
        Ok(element.bounds)
    }
}

// ---------------------------------------------------------------------------
// Android UI Tree — AccessibilityService via JNI
// ---------------------------------------------------------------------------
//
// Kotlin must call `NovaCore.nativeSetAccessibilityService(service)` after
// the AccessibilityService connects (`onServiceConnected`).
// `service` is the `android.accessibilityservice.AccessibilityService`
// instance.
//
// Tree traversal uses `getRootInActiveWindow()` and recursively walks
// `AccessibilityNodeInfo` children.  Depth limited to 32, node count to 200.

#[cfg(target_os = "android")]
use jni::objects::{GlobalRef, JObject, JValue};
#[cfg(target_os = "android")]
use jni::JNIEnv;

#[cfg(target_os = "android")]
static ACCESSIBILITY_SERVICE: std::sync::OnceLock<GlobalRef> = std::sync::OnceLock::new();

#[cfg(target_os = "android")]
pub fn set_accessibility_service(env: &JNIEnv, obj: &JObject) {
    let global = env
        .new_global_ref(obj)
        .expect("AndroidUITreeExtractor: failed to create GlobalRef for AccessibilityService");
    let _ = ACCESSIBILITY_SERVICE.set(global);
}

#[cfg(target_os = "android")]
pub fn get_accessibility_service() -> Option<&'static GlobalRef> {
    ACCESSIBILITY_SERVICE.get()
}

#[cfg(target_os = "android")]
pub fn has_accessibility_service() -> bool {
    ACCESSIBILITY_SERVICE.get().is_some()
}

#[cfg(target_os = "android")]
fn local_ref<'local>(env: &JNIEnv<'local>, global: &GlobalRef) -> crate::ScreenResult<JObject<'local>> {
    unsafe { env.new_local_ref(global.as_obj()) }
        .map_err(|_| crate::ScreenError::PlatformError("new_local_ref failed for AccessibilityService".into()))
}

#[cfg(target_os = "android")]
fn charseq_to_string(env: &JNIEnv, cs: &JObject) -> String {
    if cs.is_null() {
        return String::new();
    }
    env.call_method(cs, "toString", "()Ljava/lang/String;", &[])
        .ok()
        .and_then(|v| v.l().ok())
        .and_then(|s| {
            let js = unsafe { jni::objects::JString::from_raw(s.into_raw()) };
            env.get_string(&js).ok().map(|j| j.into())
        })
        .unwrap_or_default()
}

#[cfg(target_os = "android")]
fn class_name_to_element_type(class_name: &str) -> crate::UIElementType {
    match class_name {
        "android.widget.Button"
        | "android.widget.ImageButton"
        | "android.widget.CompoundButton" => crate::UIElementType::Button,
        "android.widget.EditText"
        | "android.widget.AutoCompleteTextView"
        | "android.widget.MultiAutoCompleteTextView" => crate::UIElementType::Edit,
        "android.widget.CheckBox" | "android.widget.Switch" => crate::UIElementType::CheckBox,
        "android.widget.Spinner"
        | "android.widget.ListView"
        | "android.widget.GridView"
        | "android.widget.ExpandableListView" => crate::UIElementType::List,
        "android.widget.ScrollView" | "android.widget.HorizontalScrollView"
        | "android.widget.FrameLayout" | "android.widget.LinearLayout"
        | "android.widget.RelativeLayout" | "android.widget.ConstraintLayout"
        | "android.view.ViewGroup" => crate::UIElementType::Pane,
        "android.widget.ImageView" | "android.widget.ImageSwitcher" => crate::UIElementType::Image,
        "android.widget.ProgressBar" | "android.widget.SeekBar"
        | "android.widget.RatingBar" => crate::UIElementType::Slider,
        "android.widget.RadioButton" => crate::UIElementType::RadioButton,
        "android.widget.TabHost" | "android.widget.TabWidget" => crate::UIElementType::Tab,
        "android.widget.Toolbar" | "android.widget.ActionMenuView"
        | "android.widget.ActionMenuPresenter" => crate::UIElementType::Toolbar,
        "android.webkit.WebView" => crate::UIElementType::Document,
        "android.widget.TextView" => crate::UIElementType::TextBlock,
        "android.widget.ListPopupWindow" | "android.widget.PopupWindow"
        | "android.widget.PopupMenu" => crate::UIElementType::Menu,
        "android.widget.StatusBar" => crate::UIElementType::StatusBar,
        s if s.contains("Button") => crate::UIElementType::Button,
        s if s.contains("EditText") => crate::UIElementType::Edit,
        s if (s.contains("Text") || s.contains("TextView")) && !s.contains("Edit") => {
            crate::UIElementType::TextBlock
        }
        s => crate::UIElementType::Custom(s.to_string()),
    }
}

/// Production-ready Android UI tree extractor backed by AccessibilityService.
#[cfg(target_os = "android")]
pub struct AndroidUITreeExtractor {
    java_vm: std::sync::Arc<jni::JavaVM>,
}

#[cfg(target_os = "android")]
impl AndroidUITreeExtractor {
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

    fn build_node(
        &self,
        env: &JNIEnv,
        node: &JObject,
        depth: u32,
        count: &mut u32,
    ) -> crate::ScreenResult<crate::UIElement> {
        const MAX_DEPTH: u32 = 32;
        const MAX_NODES: u32 = 200;

        // --- Read properties -------------------------------------------------
        let class_cs = env
            .call_method(node, "getClassName", "()Ljava/lang/CharSequence;", &[])
            .ok()
            .and_then(|v| v.l().ok());
        let class_name = class_cs
            .as_ref()
            .map(|cs| charseq_to_string(env, cs))
            .unwrap_or_default();

        let text_cs = env
            .call_method(node, "getText", "()Ljava/lang/CharSequence;", &[])
            .ok()
            .and_then(|v| v.l().ok());
        let text = text_cs
            .as_ref()
            .map(|cs| charseq_to_string(env, cs))
            .filter(|s| !s.is_empty());

        let desc_cs = env
            .call_method(node, "getContentDescription", "()Ljava/lang/CharSequence;", &[])
            .ok()
            .and_then(|v| v.l().ok());
        let content_desc = desc_cs
            .as_ref()
            .map(|cs| charseq_to_string(env, cs))
            .filter(|s| !s.is_empty());

        let pkg_cs = env
            .call_method(node, "getPackageName", "()Ljava/lang/CharSequence;", &[])
            .ok()
            .and_then(|v| v.l().ok());
        let package_name = pkg_cs
            .as_ref()
            .map(|cs| charseq_to_string(env, cs))
            .unwrap_or_default();

        // View id resource name (API 18+)
        let view_id = env
            .call_method(node, "getViewIdResourceName", "()Ljava/lang/String;", &[])
            .ok()
            .and_then(|v| v.l().ok())
            .filter(|obj| !obj.is_null())
            .map(|s| {
                let js = unsafe { jni::objects::JString::from_raw(s.into_raw()) };
                env.get_string(&js).ok().map(|j| j.into()).unwrap_or_default()
            })
            .filter(|s: &String| !s.is_empty());

        // Bounds
        let rect = env
            .new_object("android/graphics/Rect", "()V", &[])
            .unwrap_or_else(|_| JObject::null());
        if !rect.is_null() {
            env.call_method(
                node,
                "getBoundsInScreen",
                "(Landroid/graphics/Rect;)V",
                &[JValue::Object(&rect)],
            )
            .ok();
        }
        let (l, t, r, b) = if !rect.is_null() {
            let left = env.get_field(&rect, "left", "I").ok().and_then(|v| v.i().ok()).unwrap_or(0);
            let top = env.get_field(&rect, "top", "I").ok().and_then(|v| v.i().ok()).unwrap_or(0);
            let right = env.get_field(&rect, "right", "I").ok().and_then(|v| v.i().ok()).unwrap_or(0);
            let bottom = env.get_field(&rect, "bottom", "I").ok().and_then(|v| v.i().ok()).unwrap_or(0);
            (left, top, right, bottom)
        } else {
            (0, 0, 0, 0)
        };

        let bounds = crate::Rect {
            x: l,
            y: t,
            width: (r - l).max(0) as u32,
            height: (b - t).max(0) as u32,
        };

        // Attributes
        let mut attributes = std::collections::HashMap::new();
        attributes.insert("package_name".to_string(), package_name);
        if let Some(cd) = content_desc {
            attributes.insert("content_description".to_string(), cd);
        }
        if let Ok(enabled) = env
            .call_method(node, "isEnabled", "()Z", &[])
            .and_then(|v| v.z())
        {
            attributes.insert("enabled".to_string(), enabled.to_string());
        }
        if let Ok(clickable) = env
            .call_method(node, "isClickable", "()Z", &[])
            .and_then(|v| v.z())
        {
            attributes.insert("clickable".to_string(), clickable.to_string());
        }
        if let Ok(focusable) = env
            .call_method(node, "isFocusable", "()Z", &[])
            .and_then(|v| v.z())
        {
            attributes.insert("focusable".to_string(), focusable.to_string());
        }
        if let Ok(scrollable) = env
            .call_method(node, "isScrollable", "()Z", &[])
            .and_then(|v| v.z())
        {
            attributes.insert("scrollable".to_string(), scrollable.to_string());
        }

        let element_type = class_name_to_element_type(&class_name);

        let element_id = view_id.clone().unwrap_or_else(|| {
            uuid::Uuid::new_v4().to_string()
        });

        // --- Children --------------------------------------------------------
        let child_count: i32 = env
            .call_method(node, "getChildCount", "()I", &[])
            .ok()
            .and_then(|v| v.i().ok())
            .unwrap_or(0);

        let mut child_jobjects: Vec<JObject> = Vec::new();
        for i in 0..child_count.min(50) {
            if *count >= MAX_NODES {
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

        // Recycle current node before recursing into children
        env.call_method(node, "recycle", "()V", &[]).ok();

        let mut children = Vec::new();
        for child in child_jobjects {
            if depth < MAX_DEPTH && *count < MAX_NODES {
                if let Ok(child_elem) = self.build_node(env, &child, depth + 1, count) {
                    *count += 1;
                    children.push(child_elem);
                }
            }
            env.call_method(&child, "recycle", "()V", &[]).ok();
        }

        // Name = text or content_description or class_name
        let name = text
            .clone()
            .or_else(|| content_desc.clone())
            .or_else(|| {
                if !class_name.is_empty() {
                    Some(class_name.clone())
                } else {
                    None
                }
            });

        Ok(crate::UIElement {
            element_id,
            element_type,
            bounds,
            name,
            text,
            automation_id: view_id,
            class_name: Some(class_name),
            children,
            attributes,
        })
    }
}

pub fn create() -> crate::ScreenResult<std::sync::Arc<dyn crate::UITreeExtractor>> {
    #[cfg(target_os = "windows")]
    {
        Ok(std::sync::Arc::new(WindowsUITreeExtractor::new()?))
    }
    #[cfg(target_os = "android")]
    {
        Ok(std::sync::Arc::new(AndroidUITreeExtractor::new()?))
    }
    #[cfg(not(any(target_os = "windows", target_os = "android")))]
    {
        Err(crate::ScreenError::UnsupportedPlatform)
    }
}

#[async_trait]
#[cfg(target_os = "android")]
impl crate::UITreeExtractor for AndroidUITreeExtractor {
    fn id(&self) -> &str {
        "android-accessibility"
    }

    async fn extract_tree(&self, _frame: &crate::CapturedFrame) -> crate::ScreenResult<crate::UITree> {
        let as_ref = ACCESSIBILITY_SERVICE.get().ok_or(
            crate::ScreenError::PlatformError(
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
            return Err(crate::ScreenError::PlatformError(
                "getRootInActiveWindow returned null — no active window".into(),
            ));
        }

        let mut count = 1u32;
        let root_element = self.build_node(&env, &root, 0, &mut count)?;

        Ok(crate::UITree {
            root: root_element,
            timestamp: chrono::Utc::now().timestamp_millis(),
        })
    }

    async fn find_element(
        &self,
        tree: &crate::UITree,
        query: &crate::GroundingQuery,
    ) -> crate::ScreenResult<Option<crate::UIElementRef>> {
        fn find_recursive(
            element: &crate::UIElement,
            query: &crate::GroundingQuery,
        ) -> Option<crate::UIElementRef> {
            let name = element.name.as_deref().unwrap_or("");
            let text = element.text.as_deref().unwrap_or("");
            let attrs: String = element.attributes.values().cloned().collect::<Vec<_>>().join(" ");
            let combined = format!("{} {} {}", name, text, attrs).to_lowercase();
            let q = query.query.to_lowercase();
            if combined.contains(&q) {
                return Some(crate::UIElementRef {
                    element_id: element.element_id.clone(),
                    element_type: element.element_type.clone(),
                    bounds: element.bounds,
                    text: element.text.clone(),
                    attributes: element.attributes.clone(),
                });
            }
            for child in &element.children {
                if let Some(found) = find_recursive(child, query) {
                    return Some(found);
                }
            }
            None
        }
        Ok(find_recursive(&tree.root, query))
    }

    async fn get_element_bounds(
        &self,
        element: &crate::UIElementRef,
    ) -> crate::ScreenResult<Rect> {
        Ok(element.bounds)
    }
}