#[cfg(target_os = "android")]
pub mod android;
pub mod error;
pub mod events;
pub mod mock;
pub mod permission;
pub mod traits;
pub mod types;
#[cfg(target_os = "windows")]
pub mod windows;
#[cfg(target_os = "android")]
pub use android::{
    get_accessibility_service as android_get_accessibility_service,
    has_accessibility_service as android_has_accessibility_service,
    set_accessibility_service as android_set_accessibility_service, AndroidInputProvider,
};

pub use error::{InputError, InputResult};
pub use events::{InputEvent, InputEventPayload};
pub use mock::MockInputProvider;
pub use permission::{
    InputCapability, PERM_INPUT_GESTURE, PERM_INPUT_KEYBOARD, PERM_INPUT_MOUSE, PERM_INPUT_TOUCH,
};
pub use traits::InputEngine;
pub use types::*;

use async_trait::async_trait;
use nova_kernel::{
    log_activity, EventMetadata, HealthStatus, KernelModule, ModuleHealth, NovaEvent, Result,
};
use parking_lot::RwLock;
use std::collections::VecDeque;
use std::sync::Arc;

pub struct InputSystem {
    engine: RwLock<Arc<dyn InputEngine>>,
    config: RwLock<InputConfig>,
    audit: RwLock<VecDeque<InputEvent>>,
    event_bus: RwLock<Option<Arc<nova_kernel::EventBus>>>,
}

#[derive(Debug, Clone)]
pub struct InputConfig {
    pub enabled: bool,
    pub require_confirmation_for: Vec<String>,
}

impl Default for InputConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            require_confirmation_for: Vec::new(),
        }
    }
}

impl InputSystem {
    pub fn new() -> Self {
        Self {
            engine: RwLock::new(Arc::new(MockInputProvider::new())),
            config: RwLock::new(InputConfig::default()),
            audit: RwLock::new(VecDeque::with_capacity(1024)),
            event_bus: RwLock::new(None),
        }
    }

    pub fn with_engine(engine: Arc<dyn InputEngine>) -> Self {
        Self {
            engine: RwLock::new(engine),
            config: RwLock::new(InputConfig::default()),
            audit: RwLock::new(VecDeque::with_capacity(1024)),
            event_bus: RwLock::new(None),
        }
    }

    pub fn set_event_bus(&self, bus: Arc<nova_kernel::EventBus>) {
        *self.event_bus.write() = Some(bus);
    }

    pub fn set_engine(&self, engine: Arc<dyn InputEngine>) {
        *self.engine.write() = engine;
    }

    pub fn engine(&self) -> Arc<dyn InputEngine> {
        self.engine.read().clone()
    }

    pub fn audit_log(&self) -> Vec<InputEvent> {
        self.audit.read().iter().cloned().collect()
    }

    #[allow(clippy::await_holding_lock)]
    pub async fn execute(&self, action: &InputAction) -> InputResult<ActionResult> {
        let start = std::time::Instant::now();

        let cfg = self.config.read();
        if !cfg.enabled {
            let event = InputEvent::new(
                uuid::Uuid::new_v4(),
                InputEventPayload::ActionBlocked {
                    action: action.label().to_string(),
                    reason: "input system disabled".to_string(),
                },
            );
            self.push_audit_event(event.clone());
            self.publish_event(event);
            return Err(InputError::PermissionDenied(
                "input system disabled".to_string(),
            ));
        }
        drop(cfg);

        let engine = self.engine.read().clone();
        let result = engine.execute(action).await;
        let duration_ms = start.elapsed().as_millis() as u64;

        let event = match &result {
            Ok(res) => InputEvent::new(
                uuid::Uuid::new_v4(),
                InputEventPayload::ActionExecuted {
                    action: action.label().to_string(),
                    success: res.success,
                    detail: res.detail.clone(),
                    duration_ms,
                },
            ),
            Err(e) => InputEvent::new(
                uuid::Uuid::new_v4(),
                InputEventPayload::ActionFailed {
                    action: action.label().to_string(),
                    error: e.to_string(),
                },
            ),
        };

        self.push_audit_event(event.clone());
        self.publish_event(event);

        log_activity(
            "input",
            action.label(),
            &format!("duration={}ms", duration_ms),
            None,
        );

        result
    }

    pub async fn execute_batch(&self, actions: &[InputAction]) -> Vec<InputResult<ActionResult>> {
        let mut results = Vec::with_capacity(actions.len());
        for action in actions {
            results.push(self.execute(action).await);
        }
        results
    }

    pub async fn click(&self, point: types::Point) -> InputResult<ActionResult> {
        self.execute(&InputAction::Mouse(MouseAction::Click {
            point,
            button: MouseButton::Left,
            count: 1,
        }))
        .await
    }

    pub async fn double_click(&self, point: types::Point) -> InputResult<ActionResult> {
        self.execute(&InputAction::Mouse(MouseAction::Click {
            point,
            button: MouseButton::Left,
            count: 2,
        }))
        .await
    }

    pub async fn right_click(&self, point: types::Point) -> InputResult<ActionResult> {
        self.execute(&InputAction::Mouse(MouseAction::Click {
            point,
            button: MouseButton::Right,
            count: 1,
        }))
        .await
    }

    pub async fn move_mouse(&self, point: types::Point) -> InputResult<ActionResult> {
        self.execute(&InputAction::Mouse(MouseAction::Move {
            point,
            absolute: true,
        }))
        .await
    }

    pub async fn type_text(&self, text: &str) -> InputResult<ActionResult> {
        self.execute(&InputAction::Keyboard(KeyboardAction::TypeText {
            text: text.to_string(),
        }))
        .await
    }

    pub async fn press_key(&self, key: &str) -> InputResult<ActionResult> {
        self.execute(&InputAction::Keyboard(KeyboardAction::KeyPress {
            key: key.to_string(),
            modifiers: Vec::new(),
        }))
        .await
    }

    pub async fn hotkey(&self, keys: &[&str]) -> InputResult<ActionResult> {
        self.execute(&InputAction::Keyboard(KeyboardAction::Hotkey {
            keys: keys.iter().map(|k| k.to_string()).collect(),
        }))
        .await
    }

    pub async fn scroll(&self, delta_x: i32, delta_y: i32) -> InputResult<ActionResult> {
        self.execute(&InputAction::Mouse(MouseAction::Scroll {
            delta_x,
            delta_y,
        }))
        .await
    }

    pub async fn tap(&self, point: types::Point) -> InputResult<ActionResult> {
        self.execute(&InputAction::Touch(TouchAction::Tap { point }))
            .await
    }

    pub async fn swipe(&self, from: types::Point, to: types::Point) -> InputResult<ActionResult> {
        self.execute(&InputAction::Touch(TouchAction::Swipe {
            from,
            to,
            duration_ms: 100,
        }))
        .await
    }

    pub async fn drag(&self, from: types::Point, to: types::Point) -> InputResult<ActionResult> {
        self.execute(&InputAction::Mouse(MouseAction::Drag {
            from,
            to,
            button: MouseButton::Left,
        }))
        .await
    }

    fn push_audit_event(&self, event: InputEvent) {
        let mut log = self.audit.write();
        if log.len() >= 1000 {
            log.pop_front();
        }
        log.push_back(event);
    }

    fn publish_event(&self, event: InputEvent) {
        if let Some(ref bus) = *self.event_bus.read() {
            let meta = EventMetadata::new("input", Some(event.action_name().to_string()));
            let _ = bus.publish(NovaEvent {
                metadata: meta,
                payload: Arc::new(event),
            });
        }
    }
}

impl Default for InputSystem {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl KernelModule for InputSystem {
    fn module_id(&self) -> &'static str {
        "input"
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn dependencies(&self) -> Vec<&'static str> {
        Vec::new()
    }

    async fn start(&self) -> Result<()> {
        tracing::info!("Input module started");
        log_activity("input", "started", "input system started", None);
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        tracing::info!("Input module stopped");
        Ok(())
    }

    fn health(&self) -> ModuleHealth {
        let name = self.engine.read().engine_name();
        ModuleHealth {
            status: HealthStatus::Healthy,
            detail: format!("provider: {name}"),
        }
    }
}
