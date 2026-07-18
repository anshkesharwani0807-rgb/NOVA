use std::time::{SystemTime, UNIX_EPOCH};

use nova_screen::{CapturedFrame, GroundingResult, OCRResult, UITree};
use serde::{Deserialize, Serialize};

/// Configuration for the WorldState module.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldStateConfig {
    pub max_grounded_elements: usize,
    pub enable_ocr_caching: bool,
    pub enable_ui_tree_caching: bool,
    pub enable_diff_tracking: bool,
    pub redact_ocr_text: bool,
    pub redact_frame_data: bool,
    pub redact_ui_text: bool,
}

impl Default for WorldStateConfig {
    fn default() -> Self {
        Self {
            max_grounded_elements: 50,
            enable_ocr_caching: true,
            enable_ui_tree_caching: true,
            enable_diff_tracking: true,
            redact_ocr_text: false,
            redact_frame_data: false,
            redact_ui_text: false,
        }
    }
}

/// A point-in-time snapshot of the world state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldSnapshot {
    pub frame: Option<CapturedFrame>,
    pub active_app: Option<String>,
    pub ocr: Option<OCRResult>,
    pub grounded_elements: Vec<GroundingResult>,
    pub ui_tree: Option<UITree>,
    pub device_telemetry: Option<DeviceTelemetry>,
    pub network_state: Option<NetworkState>,
    pub timestamp: i64,
}

/// Live world state model aggregating screen, UI, OCR, app, device, and network state.
pub struct WorldState {
    current_frame: Option<CapturedFrame>,
    frame_updated_at: Option<i64>,
    active_app: Option<String>,
    app_updated_at: Option<i64>,
    ocr_cache: Option<OCRResult>,
    ocr_updated_at: Option<i64>,
    grounded_elements: Vec<GroundingResult>,
    elements_updated_at: Option<i64>,
    ui_tree: Option<UITree>,
    ui_tree_updated_at: Option<i64>,
    device_telemetry: Option<DeviceTelemetry>,
    network_state: Option<NetworkState>,
    previous_snapshot: Option<WorldSnapshot>,
    subscriptions: Vec<WorldSubscription>,
    next_sub_id: u64,
    config: WorldStateConfig,
}

impl WorldState {
    pub fn new() -> Self {
        Self::with_config(WorldStateConfig::default())
    }

    pub fn with_config(config: WorldStateConfig) -> Self {
        Self {
            current_frame: None,
            frame_updated_at: None,
            active_app: None,
            app_updated_at: None,
            ocr_cache: None,
            ocr_updated_at: None,
            grounded_elements: Vec::new(),
            elements_updated_at: None,
            ui_tree: None,
            ui_tree_updated_at: None,
            device_telemetry: None,
            network_state: None,
            previous_snapshot: None,
            subscriptions: Vec::new(),
            next_sub_id: 0,
            config,
        }
    }

    pub fn update_frame(&mut self, frame: CapturedFrame) {
        self.save_snapshot_before_update();
        let now = now_millis();
        self.current_frame = Some(frame);
        self.frame_updated_at = Some(now);
        self.maybe_notify();
    }

    pub fn update_active_app(&mut self, app_id: String) {
        self.save_snapshot_before_update();
        let now = now_millis();
        self.active_app = Some(app_id);
        self.app_updated_at = Some(now);
        self.maybe_notify();
    }

    pub fn update_ocr(&mut self, ocr: OCRResult) {
        if !self.config.enable_ocr_caching {
            return;
        }
        self.save_snapshot_before_update();
        let now = now_millis();
        self.ocr_cache = Some(ocr);
        self.ocr_updated_at = Some(now);
        self.maybe_notify();
    }

    pub fn update_grounded_elements(&mut self, elements: Vec<GroundingResult>) {
        self.save_snapshot_before_update();
        let now = now_millis();
        let max = self.config.max_grounded_elements;
        if elements.len() > max {
            self.grounded_elements = elements.into_iter().take(max).collect();
        } else {
            self.grounded_elements = elements;
        }
        self.elements_updated_at = Some(now);
        self.maybe_notify();
    }

    pub fn update_ui_tree(&mut self, tree: UITree) {
        if !self.config.enable_ui_tree_caching {
            return;
        }
        self.save_snapshot_before_update();
        let now = now_millis();
        self.ui_tree = Some(tree);
        self.ui_tree_updated_at = Some(now);
        self.maybe_notify();
    }

    pub fn snapshot(&self) -> WorldSnapshot {
        WorldSnapshot {
            frame: self.current_frame.clone(),
            active_app: self.active_app.clone(),
            ocr: self.ocr_cache.clone(),
            grounded_elements: self.grounded_elements.clone(),
            ui_tree: self.ui_tree.clone(),
            device_telemetry: self.device_telemetry.clone(),
            network_state: self.network_state.clone(),
            timestamp: now_millis(),
        }
    }

    pub fn config(&self) -> &WorldStateConfig {
        &self.config
    }

    pub fn config_mut(&mut self) -> &mut WorldStateConfig {
        &mut self.config
    }

    pub fn clear(&mut self) {
        self.current_frame = None;
        self.frame_updated_at = None;
        self.active_app = None;
        self.app_updated_at = None;
        self.ocr_cache = None;
        self.ocr_updated_at = None;
        self.grounded_elements.clear();
        self.elements_updated_at = None;
        self.ui_tree = None;
        self.ui_tree_updated_at = None;
        self.device_telemetry = None;
        self.network_state = None;
        self.previous_snapshot = None;
    }

    pub fn frame(&self) -> Option<&CapturedFrame> {
        self.current_frame.as_ref()
    }

    pub fn frame_updated_at(&self) -> Option<i64> {
        self.frame_updated_at
    }

    pub fn active_app(&self) -> Option<&str> {
        self.active_app.as_deref()
    }

    pub fn app_updated_at(&self) -> Option<i64> {
        self.app_updated_at
    }

    pub fn ocr_cache(&self) -> Option<&OCRResult> {
        self.ocr_cache.as_ref()
    }

    pub fn ocr_updated_at(&self) -> Option<i64> {
        self.ocr_updated_at
    }

    pub fn grounded_elements(&self) -> &[GroundingResult] {
        &self.grounded_elements
    }

    pub fn elements_updated_at(&self) -> Option<i64> {
        self.elements_updated_at
    }

    pub fn ui_tree(&self) -> Option<&UITree> {
        self.ui_tree.as_ref()
    }

    pub fn ui_tree_updated_at(&self) -> Option<i64> {
        self.ui_tree_updated_at
    }

    pub fn update_device_telemetry(&mut self, state: DeviceTelemetry) {
        self.save_snapshot_before_update();
        let now = now_millis();
        let mut s = state;
        s.last_updated = Some(now);
        self.device_telemetry = Some(s);
        self.maybe_notify();
    }

    pub fn update_network_state(&mut self, state: NetworkState) {
        self.save_snapshot_before_update();
        let now = now_millis();
        let mut s = state;
        s.last_updated = Some(now);
        self.network_state = Some(s);
        self.maybe_notify();
    }

    pub fn device_telemetry(&self) -> Option<&DeviceTelemetry> {
        self.device_telemetry.as_ref()
    }

    pub fn network_state(&self) -> Option<&NetworkState> {
        self.network_state.as_ref()
    }

    pub fn compute_diff(&self) -> Option<WorldDiff> {
        if !self.config.enable_diff_tracking {
            return None;
        }
        let prev = self.previous_snapshot.as_ref()?;
        let curr = self.current_snapshot_inner();
        Some(compare_snapshots(prev, &curr))
    }

    pub fn subscribe<F>(&mut self, callback: F) -> u64
    where
        F: Fn(&WorldSnapshot, &WorldDiff) + Send + Sync + 'static,
    {
        let id = self.next_sub_id;
        self.next_sub_id += 1;
        self.subscriptions.push(WorldSubscription {
            id,
            callback: Box::new(callback),
        });
        id
    }

    pub fn unsubscribe(&mut self, id: u64) -> bool {
        let len_before = self.subscriptions.len();
        self.subscriptions.retain(|s| s.id != id);
        self.subscriptions.len() != len_before
    }

    pub fn subscription_count(&self) -> usize {
        self.subscriptions.len()
    }

    pub fn redacted_snapshot(&self) -> WorldSnapshot {
        let mut snap = self.snapshot();
        if self.config.redact_ocr_text {
            if let Some(ref mut ocr) = snap.ocr {
                ocr.text = "[REDACTED]".into();
                for region in &mut ocr.regions {
                    region.text = "[REDACTED]".into();
                }
            }
        }
        if self.config.redact_frame_data {
            if let Some(ref mut frame) = snap.frame {
                frame.data.clear();
            }
        }
        if self.config.redact_ui_text {
            for elem in &mut snap.grounded_elements {
                elem.element.text = None;
                elem.element.attributes.clear();
                elem.match_reason = "[REDACTED]".into();
            }
        }
        snap
    }

    fn current_snapshot_inner(&self) -> WorldSnapshot {
        self.snapshot()
    }

    fn save_snapshot_before_update(&mut self) {
        if !self.config.enable_diff_tracking {
            return;
        }
        self.previous_snapshot = Some(self.snapshot());
    }

    fn maybe_notify(&self) {
        if !self.config.enable_diff_tracking || self.subscriptions.is_empty() {
            return;
        }
        let curr = self.current_snapshot_inner();
        let diff = self
            .previous_snapshot
            .as_ref()
            .map(|prev| compare_snapshots(prev, &curr))
            .unwrap_or_default();
        if !diff.has_any_change {
            return;
        }
        for sub in &self.subscriptions {
            (sub.callback)(&curr, &diff);
        }
    }
}

fn compare_snapshots(prev: &WorldSnapshot, curr: &WorldSnapshot) -> WorldDiff {
    let frame_changed = prev.frame.as_ref().map(|f| f.frame_id.as_str())
        != curr.frame.as_ref().map(|f| f.frame_id.as_str());
    let active_app_changed = prev.active_app != curr.active_app;
    let ocr_changed =
        prev.ocr.as_ref().map(|o| o.text.as_str()) != curr.ocr.as_ref().map(|o| o.text.as_str());
    let grounded_elements_changed = prev.grounded_elements.len() != curr.grounded_elements.len()
        || prev
            .grounded_elements
            .iter()
            .zip(curr.grounded_elements.iter())
            .any(|(a, b)| a.element.element_id != b.element.element_id);
    let ui_tree_changed =
        prev.ui_tree.as_ref().map(|t| t.timestamp) != curr.ui_tree.as_ref().map(|t| t.timestamp);
    let device_state_changed = prev.device_telemetry.as_ref().map(|d| d.battery_level)
        != curr.device_telemetry.as_ref().map(|d| d.battery_level)
        || prev.device_telemetry.as_ref().map(|d| d.wifi_enabled)
            != curr.device_telemetry.as_ref().map(|d| d.wifi_enabled)
        || prev.device_telemetry.as_ref().map(|d| d.bluetooth_enabled)
            != curr.device_telemetry.as_ref().map(|d| d.bluetooth_enabled);
    let network_state_changed = prev.network_state.as_ref().map(|n| n.is_online)
        != curr.network_state.as_ref().map(|n| n.is_online)
        || prev
            .network_state
            .as_ref()
            .map(|n| n.network_type.as_deref())
            != curr
                .network_state
                .as_ref()
                .map(|n| n.network_type.as_deref());

    let has_any_change = frame_changed
        || active_app_changed
        || ocr_changed
        || grounded_elements_changed
        || ui_tree_changed
        || device_state_changed
        || network_state_changed;

    WorldDiff {
        frame_changed,
        active_app_changed,
        ocr_changed,
        grounded_elements_changed,
        ui_tree_changed,
        device_state_changed,
        network_state_changed,
        has_any_change,
    }
}

impl Default for WorldState {
    fn default() -> Self {
        Self::new()
    }
}

/// Device-level state (battery, Wi-Fi, Bluetooth).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceTelemetry {
    pub battery_level: Option<u8>,
    pub is_charging: Option<bool>,
    pub wifi_enabled: Option<bool>,
    pub bluetooth_enabled: Option<bool>,
    pub last_updated: Option<i64>,
}

impl DeviceTelemetry {
    pub fn new() -> Self {
        Self {
            battery_level: None,
            is_charging: None,
            wifi_enabled: None,
            bluetooth_enabled: None,
            last_updated: None,
        }
    }
}

impl Default for DeviceTelemetry {
    fn default() -> Self {
        Self::new()
    }
}

/// Network connectivity state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkState {
    pub is_online: Option<bool>,
    pub network_type: Option<String>,
    pub last_updated: Option<i64>,
}

impl NetworkState {
    pub fn new() -> Self {
        Self {
            is_online: None,
            network_type: None,
            last_updated: None,
        }
    }
}

impl Default for NetworkState {
    fn default() -> Self {
        Self::new()
    }
}

/// Describes what changed between two world state snapshots.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldDiff {
    pub frame_changed: bool,
    pub active_app_changed: bool,
    pub ocr_changed: bool,
    pub grounded_elements_changed: bool,
    pub ui_tree_changed: bool,
    pub device_state_changed: bool,
    pub network_state_changed: bool,
    pub has_any_change: bool,
}

impl WorldDiff {
    pub fn new() -> Self {
        Self {
            frame_changed: false,
            active_app_changed: false,
            ocr_changed: false,
            grounded_elements_changed: false,
            ui_tree_changed: false,
            device_state_changed: false,
            network_state_changed: false,
            has_any_change: false,
        }
    }
}

impl Default for WorldDiff {
    fn default() -> Self {
        Self::new()
    }
}

/// A registered subscription that receives notifications on world state changes.
pub struct WorldSubscription {
    id: u64,
    #[allow(clippy::type_complexity)]
    callback: Box<dyn Fn(&WorldSnapshot, &WorldDiff) + Send + Sync>,
}

impl std::fmt::Debug for WorldSubscription {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorldSubscription")
            .field("id", &self.id)
            .finish()
    }
}

impl WorldSubscription {
    pub fn id(&self) -> u64 {
        self.id
    }
}

/// Platform-agnostic trait for collecting device and network state.
/// Implementations exist for Windows and Android with graceful fallback.
pub trait DeviceTelemetryCollector: Send + Sync {
    fn collect_device_telemetry(&self) -> DeviceTelemetry;
    fn collect_network_state(&self) -> NetworkState;
}

/// A collector that returns unknown state for all fields (graceful fallback).
#[derive(Debug, Clone)]
pub struct NullDeviceTelemetryCollector;

impl DeviceTelemetryCollector for NullDeviceTelemetryCollector {
    fn collect_device_telemetry(&self) -> DeviceTelemetry {
        DeviceTelemetry::new()
    }

    fn collect_network_state(&self) -> NetworkState {
        NetworkState::new()
    }
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nova_screen::{OCRRegion, PixelFormat, Rect, UIElement, UIElementRef, UIElementType};

    fn dummy_frame() -> CapturedFrame {
        CapturedFrame {
            frame_id: "frame_001".into(),
            timestamp: now_millis(),
            width: 1920,
            height: 1080,
            format: PixelFormat::RGBA8,
            data: vec![0u8; 64],
            region: None,
        }
    }

    fn dummy_ui_tree() -> UITree {
        UITree {
            root: UIElement {
                element_id: "root".into(),
                element_type: UIElementType::Window,
                bounds: Rect {
                    x: 0,
                    y: 0,
                    width: 1920,
                    height: 1080,
                },
                name: Some("Desktop".into()),
                text: None,
                automation_id: None,
                class_name: Some("Window".into()),
                children: vec![],
                attributes: std::collections::HashMap::new(),
            },
            timestamp: now_millis(),
        }
    }

    fn dummy_ocr_result() -> OCRResult {
        OCRResult {
            text: "Hello World".into(),
            confidence: 0.95,
            language: "en".into(),
            regions: vec![OCRRegion {
                text: "Hello".into(),
                confidence: 0.95,
                bounds: Rect {
                    x: 10,
                    y: 10,
                    width: 50,
                    height: 20,
                },
            }],
        }
    }

    fn dummy_grounding_result(text: &str) -> GroundingResult {
        GroundingResult {
            element: UIElementRef {
                element_id: format!("elem_{}", text),
                element_type: UIElementType::Button,
                bounds: Rect {
                    x: 100,
                    y: 200,
                    width: 80,
                    height: 30,
                },
                text: Some(text.into()),
                attributes: std::collections::HashMap::new(),
            },
            confidence: 0.9,
            match_reason: format!("matched text '{}'", text),
        }
    }

    #[test]
    fn test_default_state_is_empty() {
        let ws = WorldState::new();
        assert!(ws.frame().is_none());
        assert!(ws.active_app().is_none());
        assert!(ws.ocr_cache().is_none());
        assert!(ws.grounded_elements().is_empty());
        assert!(ws.ui_tree().is_none());
        assert!(ws.frame_updated_at().is_none());
        assert!(ws.app_updated_at().is_none());
        assert!(ws.ocr_updated_at().is_none());
        assert!(ws.elements_updated_at().is_none());
        assert!(ws.ui_tree_updated_at().is_none());
    }

    #[test]
    fn test_update_frame_stores_frame() {
        let mut ws = WorldState::new();
        let frame = dummy_frame();
        ws.update_frame(frame.clone());
        assert!(ws.frame().is_some());
        assert_eq!(ws.frame().unwrap().frame_id, "frame_001");
        assert!(ws.frame_updated_at().is_some());
    }

    #[test]
    fn test_update_active_app_stores_app() {
        let mut ws = WorldState::new();
        ws.update_active_app("calculator".into());
        assert_eq!(ws.active_app(), Some("calculator"));
        assert!(ws.app_updated_at().is_some());
    }

    #[test]
    fn test_update_ocr_stores_result() {
        let mut ws = WorldState::new();
        let ocr = dummy_ocr_result();
        ws.update_ocr(ocr.clone());
        assert!(ws.ocr_cache().is_some());
        assert_eq!(ws.ocr_cache().unwrap().text, "Hello World");
        assert!(ws.ocr_updated_at().is_some());
    }

    #[test]
    fn test_update_ocr_skipped_when_caching_disabled() {
        let config = WorldStateConfig {
            enable_ocr_caching: false,
            ..Default::default()
        };
        let mut ws = WorldState::with_config(config);
        ws.update_ocr(dummy_ocr_result());
        assert!(ws.ocr_cache().is_none());
    }

    #[test]
    fn test_update_grounded_elements_stores_elements() {
        let mut ws = WorldState::new();
        let elements = vec![
            dummy_grounding_result("OK"),
            dummy_grounding_result("Cancel"),
        ];
        ws.update_grounded_elements(elements.clone());
        assert_eq!(ws.grounded_elements().len(), 2);
        assert!(ws.elements_updated_at().is_some());
    }

    #[test]
    fn test_update_grounded_elements_caps_at_max() {
        let config = WorldStateConfig {
            max_grounded_elements: 3,
            ..Default::default()
        };
        let mut ws = WorldState::with_config(config);
        let elements: Vec<GroundingResult> = (0..10)
            .map(|i| dummy_grounding_result(&format!("item_{}", i)))
            .collect();
        ws.update_grounded_elements(elements);
        assert_eq!(ws.grounded_elements().len(), 3);
    }

    #[test]
    fn test_update_ui_tree_stores_tree() {
        let mut ws = WorldState::new();
        let tree = dummy_ui_tree();
        ws.update_ui_tree(tree.clone());
        assert!(ws.ui_tree().is_some());
        assert_eq!(
            ws.ui_tree().unwrap().root.element_type,
            UIElementType::Window
        );
        assert!(ws.ui_tree_updated_at().is_some());
    }

    #[test]
    fn test_update_ui_tree_skipped_when_caching_disabled() {
        let config = WorldStateConfig {
            enable_ui_tree_caching: false,
            ..Default::default()
        };
        let mut ws = WorldState::with_config(config);
        ws.update_ui_tree(dummy_ui_tree());
        assert!(ws.ui_tree().is_none());
    }

    #[test]
    fn test_snapshot_captures_current_state() {
        let mut ws = WorldState::new();
        ws.update_active_app("settings".into());
        let frame = dummy_frame();
        ws.update_frame(frame.clone());
        let elements = vec![dummy_grounding_result("Save")];
        ws.update_grounded_elements(elements.clone());

        let snap = ws.snapshot();
        assert_eq!(snap.active_app, Some("settings".into()));
        assert!(snap.frame.is_some());
        assert_eq!(snap.frame.unwrap().frame_id, "frame_001");
        assert_eq!(snap.grounded_elements.len(), 1);
        assert!(snap.ocr.is_none());
        assert!(snap.ui_tree.is_none());
    }

    #[test]
    fn test_clear_resets_all_state() {
        let mut ws = WorldState::new();
        ws.update_active_app("chrome".into());
        ws.update_frame(dummy_frame());
        ws.update_ocr(dummy_ocr_result());
        ws.update_grounded_elements(vec![dummy_grounding_result("btn")]);
        ws.update_ui_tree(dummy_ui_tree());

        ws.clear();
        assert!(ws.frame().is_none());
        assert!(ws.active_app().is_none());
        assert!(ws.ocr_cache().is_none());
        assert!(ws.grounded_elements().is_empty());
        assert!(ws.ui_tree().is_none());
        assert!(ws.frame_updated_at().is_none());
        assert!(ws.app_updated_at().is_none());
        assert!(ws.ocr_updated_at().is_none());
        assert!(ws.elements_updated_at().is_none());
        assert!(ws.ui_tree_updated_at().is_none());
    }

    #[test]
    fn test_snapshot_contains_timestamp() {
        let ws = WorldState::new();
        let snap = ws.snapshot();
        assert!(snap.timestamp > 0);
    }

    #[test]
    fn test_config_accessors() {
        let config = WorldStateConfig {
            max_grounded_elements: 10,
            enable_ocr_caching: false,
            enable_ui_tree_caching: false,
            ..Default::default()
        };
        let ws = WorldState::with_config(config.clone());
        assert_eq!(ws.config().max_grounded_elements, 10);
        assert!(!ws.config().enable_ocr_caching);
        assert!(!ws.config().enable_ui_tree_caching);
    }

    #[test]
    fn test_config_mut_modifies_config() {
        let mut ws = WorldState::new();
        ws.config_mut().max_grounded_elements = 99;
        assert_eq!(ws.config().max_grounded_elements, 99);
    }

    #[test]
    fn test_default_config_values() {
        let config = WorldStateConfig::default();
        assert_eq!(config.max_grounded_elements, 50);
        assert!(config.enable_ocr_caching);
        assert!(config.enable_ui_tree_caching);
    }

    #[test]
    fn test_update_frame_overwrites_previous() {
        let mut ws = WorldState::new();
        let frame1 = CapturedFrame {
            frame_id: "first".into(),
            ..dummy_frame()
        };
        let frame2 = CapturedFrame {
            frame_id: "second".into(),
            ..dummy_frame()
        };
        ws.update_frame(frame1);
        ws.update_frame(frame2);
        assert_eq!(ws.frame().unwrap().frame_id, "second");
    }

    #[test]
    fn test_update_active_app_overwrites_previous() {
        let mut ws = WorldState::new();
        ws.update_active_app("app1".into());
        ws.update_active_app("app2".into());
        assert_eq!(ws.active_app(), Some("app2"));
    }

    #[test]
    fn test_with_config_uses_custom_config() {
        let config = WorldStateConfig {
            max_grounded_elements: 5,
            enable_ocr_caching: false,
            enable_ui_tree_caching: false,
            ..Default::default()
        };
        let ws = WorldState::with_config(config);
        assert_eq!(ws.config().max_grounded_elements, 5);
        assert!(!ws.config().enable_ocr_caching);
    }

    #[test]
    fn test_grounded_elements_replaces_previous() {
        let mut ws = WorldState::new();
        ws.update_grounded_elements(vec![dummy_grounding_result("first")]);
        assert_eq!(ws.grounded_elements().len(), 1);
        ws.update_grounded_elements(vec![
            dummy_grounding_result("a"),
            dummy_grounding_result("b"),
        ]);
        assert_eq!(ws.grounded_elements().len(), 2);
        assert_eq!(ws.grounded_elements()[0].element.text.as_deref(), Some("a"));
    }

    #[test]
    fn test_update_ocr_overwrites_previous() {
        let mut ws = WorldState::new();
        let ocr1 = OCRResult {
            text: "first".into(),
            ..dummy_ocr_result()
        };
        let ocr2 = OCRResult {
            text: "second".into(),
            ..dummy_ocr_result()
        };
        ws.update_ocr(ocr1);
        ws.update_ocr(ocr2);
        assert_eq!(ws.ocr_cache().unwrap().text, "second");
    }

    #[test]
    fn test_update_ui_tree_overwrites_previous() {
        let mut ws = WorldState::new();
        let tree1 = UITree {
            root: UIElement {
                element_id: "old".into(),
                ..dummy_ui_tree().root
            },
            ..dummy_ui_tree()
        };
        let tree2 = UITree {
            root: UIElement {
                element_id: "new".into(),
                ..dummy_ui_tree().root
            },
            ..dummy_ui_tree()
        };
        ws.update_ui_tree(tree1);
        ws.update_ui_tree(tree2);
        assert_eq!(ws.ui_tree().unwrap().root.element_id, "new");
    }

    #[test]
    fn test_snapshot_independent_from_live_state() {
        let mut ws = WorldState::new();
        ws.update_active_app("original".into());
        let snap = ws.snapshot();
        ws.update_active_app("modified".into());
        assert_eq!(snap.active_app, Some("original".into()));
        assert_eq!(ws.active_app(), Some("modified"));
    }

    // --- Device State ---

    #[test]
    fn test_device_state_default_is_empty() {
        let ds = DeviceTelemetry::new();
        assert!(ds.battery_level.is_none());
        assert!(ds.is_charging.is_none());
        assert!(ds.wifi_enabled.is_none());
        assert!(ds.bluetooth_enabled.is_none());
        assert!(ds.last_updated.is_none());
    }

    #[test]
    fn test_update_device_state_stores_state() {
        let mut ws = WorldState::new();
        let ds = DeviceTelemetry {
            battery_level: Some(85),
            is_charging: Some(true),
            wifi_enabled: Some(true),
            bluetooth_enabled: Some(false),
            last_updated: None,
        };
        ws.update_device_telemetry(ds);
        assert!(ws.device_telemetry().is_some());
        assert_eq!(ws.device_telemetry().unwrap().battery_level, Some(85));
        assert_eq!(ws.device_telemetry().unwrap().is_charging, Some(true));
        assert!(ws.device_telemetry().unwrap().last_updated.is_some());
    }

    #[test]
    fn test_update_device_state_overwrites() {
        let mut ws = WorldState::new();
        ws.update_device_telemetry(DeviceTelemetry {
            battery_level: Some(50),
            ..DeviceTelemetry::new()
        });
        ws.update_device_telemetry(DeviceTelemetry {
            battery_level: Some(90),
            ..DeviceTelemetry::new()
        });
        assert_eq!(ws.device_telemetry().unwrap().battery_level, Some(90));
    }

    #[test]
    fn test_device_state_getter_reflects_updates() {
        let mut ws = WorldState::new();
        assert!(ws.device_telemetry().is_none());
        ws.update_device_telemetry(DeviceTelemetry {
            wifi_enabled: Some(false),
            ..DeviceTelemetry::new()
        });
        assert_eq!(ws.device_telemetry().unwrap().wifi_enabled, Some(false));
    }

    // --- Network State ---

    #[test]
    fn test_network_state_default_is_empty() {
        let ns = NetworkState::new();
        assert!(ns.is_online.is_none());
        assert!(ns.network_type.is_none());
        assert!(ns.last_updated.is_none());
    }

    #[test]
    fn test_update_network_state_stores_state() {
        let mut ws = WorldState::new();
        ws.update_network_state(NetworkState {
            is_online: Some(true),
            network_type: Some("wifi".into()),
            last_updated: None,
        });
        assert!(ws.network_state().is_some());
        assert_eq!(ws.network_state().unwrap().is_online, Some(true));
        assert_eq!(
            ws.network_state().unwrap().network_type.as_deref(),
            Some("wifi")
        );
        assert!(ws.network_state().unwrap().last_updated.is_some());
    }

    #[test]
    fn test_update_network_state_overwrites() {
        let mut ws = WorldState::new();
        ws.update_network_state(NetworkState {
            is_online: Some(false),
            ..NetworkState::new()
        });
        ws.update_network_state(NetworkState {
            is_online: Some(true),
            ..NetworkState::new()
        });
        assert_eq!(ws.network_state().unwrap().is_online, Some(true));
    }

    #[test]
    fn test_network_state_getter() {
        let mut ws = WorldState::new();
        assert!(ws.network_state().is_none());
        ws.update_network_state(NetworkState {
            network_type: Some("ethernet".into()),
            ..NetworkState::new()
        });
        assert_eq!(
            ws.network_state().unwrap().network_type.as_deref(),
            Some("ethernet")
        );
    }

    // --- Snapshot with device/network state ---

    #[test]
    fn test_snapshot_includes_device_and_network_state() {
        let mut ws = WorldState::new();
        ws.update_device_telemetry(DeviceTelemetry {
            battery_level: Some(42),
            ..DeviceTelemetry::new()
        });
        ws.update_network_state(NetworkState {
            is_online: Some(true),
            ..NetworkState::new()
        });
        let snap = ws.snapshot();
        assert!(snap.device_telemetry.is_some());
        assert_eq!(snap.device_telemetry.unwrap().battery_level, Some(42));
        assert!(snap.network_state.is_some());
        assert_eq!(snap.network_state.unwrap().is_online, Some(true));
    }

    // --- Diff Tracking ---

    #[test]
    fn test_compute_diff_returns_none_when_disabled() {
        let config = WorldStateConfig {
            enable_diff_tracking: false,
            ..Default::default()
        };
        let mut ws = WorldState::with_config(config);
        ws.update_active_app("test".into());
        assert!(ws.compute_diff().is_none());
    }

    #[test]
    fn test_compute_diff_returns_some_when_enabled() {
        let mut ws = WorldState::new();
        ws.update_active_app("first".into());
        // First update sets previous_snapshot; second triggers comparison.
        ws.update_active_app("second".into());
        let diff = ws.compute_diff();
        assert!(diff.is_some());
        assert!(diff.unwrap().active_app_changed);
    }

    #[test]
    fn test_compute_diff_no_changes_when_idempotent() {
        let mut ws = WorldState::new();
        ws.update_active_app("app".into());
        ws.update_active_app("app".into());
        let diff = ws.compute_diff();
        assert!(diff.is_some());
        assert!(!diff.unwrap().active_app_changed);
    }

    #[test]
    fn test_compute_diff_detects_frame_change() {
        let mut ws = WorldState::new();
        ws.update_frame(dummy_frame());
        let mut f2 = dummy_frame();
        f2.frame_id = "other".into();
        ws.update_frame(f2);
        let diff = ws.compute_diff();
        assert!(diff.unwrap().frame_changed);
    }

    #[test]
    fn test_compute_diff_detects_device_change() {
        let mut ws = WorldState::new();
        ws.update_device_telemetry(DeviceTelemetry {
            battery_level: Some(80),
            ..DeviceTelemetry::new()
        });
        ws.update_device_telemetry(DeviceTelemetry {
            battery_level: Some(30),
            ..DeviceTelemetry::new()
        });
        let diff = ws.compute_diff();
        assert!(diff.unwrap().device_state_changed);
    }

    // --- Subscriptions ---

    #[test]
    fn test_subscribe_receives_notification() {
        let mut ws = WorldState::new();
        let notified = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let n = notified.clone();
        let _id = ws.subscribe(move |_, _| {
            n.store(true, std::sync::atomic::Ordering::SeqCst);
        });
        ws.update_active_app("a".into());
        ws.update_active_app("b".into());
        assert!(notified.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[test]
    fn test_subscribe_receives_correct_diff() {
        let mut ws = WorldState::new();
        let captured = std::sync::Arc::new(std::sync::Mutex::new(None::<WorldDiff>));
        let c = captured.clone();
        let _id = ws.subscribe(move |_, diff| {
            *c.lock().unwrap() = Some(diff.clone());
        });
        ws.update_active_app("first".into());
        ws.update_network_state(NetworkState {
            is_online: Some(true),
            ..NetworkState::new()
        });
        let diff = captured.lock().unwrap();
        assert!(diff.is_some());
        assert!(diff.as_ref().unwrap().network_state_changed);
    }

    #[test]
    fn test_unsubscribe_stops_notification() {
        let mut ws = WorldState::new();
        let count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let c = count.clone();
        let id = ws.subscribe(move |_, _| {
            c.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        });
        ws.update_active_app("x".into());
        ws.update_active_app("y".into());
        let first_count = count.load(std::sync::atomic::Ordering::SeqCst);
        ws.unsubscribe(id);
        ws.update_active_app("z".into());
        assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), first_count);
    }

    #[test]
    fn test_subscription_count() {
        let mut ws = WorldState::new();
        assert_eq!(ws.subscription_count(), 0);
        let _id1 = ws.subscribe(|_, _| {});
        assert_eq!(ws.subscription_count(), 1);
        let _id2 = ws.subscribe(|_, _| {});
        assert_eq!(ws.subscription_count(), 2);
    }

    #[test]
    fn test_subscribe_returns_unique_ids() {
        let mut ws = WorldState::new();
        let id1 = ws.subscribe(|_, _| {});
        let id2 = ws.subscribe(|_, _| {});
        assert_ne!(id1, id2);
    }

    // --- Permissions & Privacy ---

    #[test]
    fn test_redacted_snapshot_redacts_ocr_text() {
        let config = WorldStateConfig {
            redact_ocr_text: true,
            ..Default::default()
        };
        let mut ws = WorldState::with_config(config);
        ws.update_ocr(dummy_ocr_result());
        let snap = ws.redacted_snapshot();
        assert_eq!(snap.ocr.unwrap().text, "[REDACTED]");
    }

    #[test]
    fn test_redacted_snapshot_redacts_frame_data() {
        let config = WorldStateConfig {
            redact_frame_data: true,
            ..Default::default()
        };
        let mut ws = WorldState::with_config(config);
        ws.update_frame(dummy_frame());
        let snap = ws.redacted_snapshot();
        assert!(snap.frame.unwrap().data.is_empty());
    }

    #[test]
    fn test_redacted_snapshot_does_not_affect_live_state() {
        let config = WorldStateConfig {
            redact_ocr_text: true,
            ..Default::default()
        };
        let mut ws = WorldState::with_config(config);
        ws.update_ocr(dummy_ocr_result());
        let _snap = ws.redacted_snapshot();
        assert_eq!(ws.ocr_cache().unwrap().text, "Hello World");
    }

    #[test]
    fn test_redacted_snapshot_no_redaction_by_default() {
        let mut ws = WorldState::new();
        ws.update_ocr(dummy_ocr_result());
        ws.update_frame(dummy_frame());
        ws.update_grounded_elements(vec![dummy_grounding_result("Save")]);
        let snap = ws.redacted_snapshot();
        assert_eq!(snap.ocr.unwrap().text, "Hello World");
        assert!(!snap.frame.unwrap().data.is_empty());
    }

    // --- DeviceStateCollector ---

    #[test]
    fn test_null_collector_returns_empty_state() {
        let collector = NullDeviceTelemetryCollector;
        let ds = collector.collect_device_telemetry();
        assert!(ds.battery_level.is_none());
        assert!(ds.is_charging.is_none());
        assert!(ds.wifi_enabled.is_none());
        assert!(ds.bluetooth_enabled.is_none());
        let ns = collector.collect_network_state();
        assert!(ns.is_online.is_none());
        assert!(ns.network_type.is_none());
    }

    // --- WorldDiff ---

    #[test]
    fn test_world_diff_default_has_no_changes() {
        let diff = WorldDiff::new();
        assert!(!diff.frame_changed);
        assert!(!diff.active_app_changed);
        assert!(!diff.ocr_changed);
        assert!(!diff.grounded_elements_changed);
        assert!(!diff.ui_tree_changed);
        assert!(!diff.device_state_changed);
        assert!(!diff.network_state_changed);
        assert!(!diff.has_any_change);
    }

    // --- Clear also resets new fields ---

    #[test]
    fn test_clear_resets_device_and_network_state() {
        let mut ws = WorldState::new();
        ws.update_device_telemetry(DeviceTelemetry {
            battery_level: Some(75),
            ..DeviceTelemetry::new()
        });
        ws.update_network_state(NetworkState {
            is_online: Some(false),
            ..NetworkState::new()
        });
        ws.clear();
        assert!(ws.device_telemetry().is_none());
        assert!(ws.network_state().is_none());
    }
}
