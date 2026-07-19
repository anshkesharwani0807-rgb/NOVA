use std::sync::Arc;
use std::time::Duration;

use nova_screen::engine::ScreenEngine;
use nova_screen::{CapturedFrame, GroundingQuery, GroundingResult, OCRResult};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use crate::action::ActionResult;
use crate::pipeline_step::{PipelineStep, VerificationStrategy};
use crate::world_state::{WorldDiff, WorldSnapshot, WorldState};

/// The result of verifying a pipeline step's outcome.
///
/// # Variants
/// - `Passed` — the step achieved its intended outcome.
/// - `Failed` — the step did not achieve its intended outcome; includes a
///   human-readable reason and a suggestion for recovery.
/// - `Uncertain` — verification could not determine success or failure with
///   confidence (e.g., screen engine unavailable, timeout).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum VerificationResult {
    /// Step outcome confirmed successful.
    Passed,
    /// Step outcome failed with a reason and suggested action.
    Failed {
        /// Human-readable explanation of what went wrong.
        reason: String,
        /// Suggested action for recovery or diagnosis.
        suggestion: String,
    },
    /// Verification could not determine outcome with confidence.
    Uncertain {
        /// Reason why verification was inconclusive.
        reason: String,
    },
}

impl VerificationResult {
    /// Returns `true` if the verification passed.
    pub fn is_passed(&self) -> bool {
        matches!(self, Self::Passed)
    }

    /// Returns `true` if the verification definitively failed.
    pub fn is_failed(&self) -> bool {
        matches!(self, Self::Failed { .. })
    }

    /// Returns `true` if the verification was inconclusive.
    pub fn is_uncertain(&self) -> bool {
        matches!(self, Self::Uncertain { .. })
    }

    /// Extract a human-readable summary string.
    pub fn summary(&self) -> String {
        match self {
            Self::Passed => "PASSED".to_string(),
            Self::Failed { reason, suggestion } => {
                format!("FAILED: {}. Suggestion: {}", reason, suggestion)
            }
            Self::Uncertain { reason } => format!("UNCERTAIN: {}", reason),
        }
    }
}

/// Evidence collected during the verification of a single pipeline step.
///
/// Suitable for audit logging, debugging, and activity trail recording.
#[derive(Debug, Clone, Default)]
pub struct VerificationEvidence {
    /// World snapshot captured before step execution (if provided).
    pub pre_snapshot: Option<WorldSnapshot>,
    /// World snapshot captured during or after verification.
    pub post_snapshot: Option<WorldSnapshot>,
    /// Computed diff between pre and post snapshots.
    pub world_diff: Option<WorldDiff>,
    /// Frame captured during verification (if screen engine was available).
    pub captured_frame: Option<CapturedFrame>,
    /// OCR result from captured frame (if OCR was run).
    pub ocr_result: Option<OCRResult>,
    /// Grounded elements found during verification.
    pub grounded_elements: Vec<GroundingResult>,
}

impl VerificationEvidence {
    /// Create an empty evidence set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns `true` if any evidence was collected.
    pub fn has_any(&self) -> bool {
        self.pre_snapshot.is_some()
            || self.post_snapshot.is_some()
            || self.world_diff.is_some()
            || self.captured_frame.is_some()
            || self.ocr_result.is_some()
            || !self.grounded_elements.is_empty()
    }
}

/// Verifies pipeline step execution outcomes using the WorldState and
/// optional ScreenEngine.
///
/// The verifier supports five verification modes mapped from
/// [`VerificationStrategy`]:
///
/// | Strategy | Method | Dependencies |
/// |---|---|---|
/// | `NoVerification` | Always passes | None |
/// | `OCRTextPresent` | `verify_screen_contains` | ScreenEngine |
/// | `AppInForeground` | `verify_active_app_changed` | WorldState |
/// | `DeviceTelemetryMatch` | `verify_device_state` | WorldState |
/// | `CompareSnapshots` | `verify_world_state_diff` | WorldState (pre snapshot) |
///
/// When the ScreenEngine is unavailable (`None`), screen-based verification
/// returns [`VerificationResult::Uncertain`] with an explanation.
pub struct OutcomeVerifier {
    world_state: Arc<RwLock<WorldState>>,
    screen: Option<Arc<RwLock<ScreenEngine>>>,
    default_timeout: Duration,
}

impl OutcomeVerifier {
    /// Create a new `OutcomeVerifier`.
    ///
    /// `world_state` — shared reference to the live WorldState, used for
    /// device telemetry, active app, and snapshot-diff verification.
    ///
    /// `screen` — optional shared reference to a ScreenEngine for screen-based
    /// verification (OCR, grounding, frame capture). Pass `None` when screen
    /// capture is not available; screen-based checks will return `Uncertain`.
    pub fn new(
        world_state: Arc<RwLock<WorldState>>,
        screen: Option<Arc<RwLock<ScreenEngine>>>,
    ) -> Self {
        Self {
            world_state,
            screen,
            default_timeout: Duration::from_secs(10),
        }
    }

    /// Set the default timeout for screen-capture and OCR operations.
    pub fn with_default_timeout(mut self, timeout: Duration) -> Self {
        self.default_timeout = timeout;
        self
    }

    /// Verify the outcome of a completed pipeline step.
    ///
    /// Dispatches to the appropriate verification method based on the step's
    /// [`VerificationStrategy`]. Collects evidence (pre/post snapshots, frame,
    /// OCR, diff) during verification.
    ///
    /// If `action_result` indicates a failed action execution, verification
    /// returns `Failed` immediately without further checking.
    ///
    /// `pre_snapshot` — the world snapshot captured before step execution,
    /// used by `CompareSnapshots` to detect changes.
    pub async fn verify(
        &self,
        step: &PipelineStep,
        action_result: &ActionResult,
        pre_snapshot: Option<&WorldSnapshot>,
    ) -> (VerificationResult, VerificationEvidence) {
        if !action_result.success {
            let evidence = VerificationEvidence {
                pre_snapshot: pre_snapshot.cloned(),
                ..Default::default()
            };
            return (
                VerificationResult::Failed {
                    reason: format!("action execution failed: {}", action_result.message),
                    suggestion: "check action parameters and retry".into(),
                },
                evidence,
            );
        }

        let mut evidence = VerificationEvidence {
            pre_snapshot: pre_snapshot.cloned(),
            ..Default::default()
        };

        {
            let ws = self.world_state.read();
            let post = ws.snapshot();
            if let Some(pre) = pre_snapshot {
                evidence.world_diff = Some(compare_snapshots(pre, &post));
            }
            evidence.post_snapshot = Some(post);
        }

        let result = match &step.verification {
            VerificationStrategy::NoVerification => VerificationResult::Passed,
            VerificationStrategy::OCRTextPresent { expected_text } => {
                self.verify_screen_contains(expected_text).await
            }
            VerificationStrategy::AppInForeground { app_name } => {
                self.verify_active_app_changed(app_name).await
            }
            VerificationStrategy::DeviceTelemetryMatch { field, expected } => {
                self.verify_device_state(field, expected, pre_snapshot)
                    .await
            }
            VerificationStrategy::CompareSnapshots => match pre_snapshot {
                Some(_) => self.verify_world_state_diff(pre_snapshot.unwrap()).await,
                None => VerificationResult::Uncertain {
                    reason: "no pre-execution snapshot available for comparison".into(),
                },
            },
        };

        (result, evidence)
    }

    /// Verify that a specific text string is visible on screen via OCR.
    ///
    /// Returns `Uncertain` if the ScreenEngine is not available or if
    /// capture/OCR fails.
    pub async fn verify_screen_contains(&self, text: &str) -> VerificationResult {
        let screen = match &self.screen {
            Some(s) => s,
            None => {
                return VerificationResult::Uncertain {
                    reason: "screen engine not available for OCR verification".into(),
                }
            }
        };

        let frame = match capture_frame_with_timeout(screen, self.default_timeout).await {
            Ok(f) => f,
            Err(e) => {
                return VerificationResult::Uncertain {
                    reason: format!("frame capture failed: {}", e),
                }
            }
        };

        let ocr_result = match ocr_frame_with_timeout(screen, &frame, self.default_timeout).await {
            Ok(o) => o,
            Err(e) => {
                return VerificationResult::Uncertain {
                    reason: format!("OCR failed: {}", e),
                }
            }
        };

        if ocr_result.text.contains(text) {
            VerificationResult::Passed
        } else {
            VerificationResult::Failed {
                reason: format!("expected text '{}' not found on screen", text),
                suggestion: concat!(
                    "check if the screen changed as expected, ",
                    "or if the text appears in a different region"
                )
                .into(),
            }
        }
    }

    /// Verify that a UI element matching a query exists on screen via grounding.
    ///
    /// Returns `Uncertain` if the ScreenEngine is not available or if
    /// capture/grounding fails.
    pub async fn verify_element_exists(&self, query: &str) -> VerificationResult {
        let screen = match &self.screen {
            Some(s) => s,
            None => {
                return VerificationResult::Uncertain {
                    reason: "screen engine not available for element grounding".into(),
                }
            }
        };

        let frame = match capture_frame_with_timeout(screen, self.default_timeout).await {
            Ok(f) => f,
            Err(e) => {
                return VerificationResult::Uncertain {
                    reason: format!("frame capture failed: {}", e),
                }
            }
        };

        let grounding_query = GroundingQuery {
            query: query.to_string(),
            context: None,
            max_results: 1,
            confidence_threshold: 0.3,
        };

        let result = match ground_element_with_timeout(
            screen,
            &frame,
            &grounding_query,
            self.default_timeout,
        )
        .await
        {
            Ok(r) => r,
            Err(e) => {
                return VerificationResult::Uncertain {
                    reason: format!("element grounding failed: {}", e),
                }
            }
        };

        if result.confidence >= 0.3 {
            VerificationResult::Passed
        } else {
            VerificationResult::Failed {
                reason: format!("element '{}' not found on screen", query),
                suggestion: "try a different query or check if the element is visible".into(),
            }
        }
    }

    /// Verify that the screen text content changed between pre- and post-
    /// execution snapshots.
    ///
    /// Compares the OCR text from the pre-snapshot (if available) with the
    /// current OCR text in WorldState.  If neither snapshot has OCR data,
    /// falls back to checking the WorldDiff for an OCR change.
    pub async fn verify_text_changed(&self, pre_snapshot: &WorldSnapshot) -> VerificationResult {
        let ws = self.world_state.read();
        let current = ws.snapshot();
        drop(ws);

        let pre_text = pre_snapshot
            .ocr
            .as_ref()
            .map(|o| o.text.as_str())
            .unwrap_or("");
        let post_text = current.ocr.as_ref().map(|o| o.text.as_str()).unwrap_or("");

        if pre_text != post_text {
            return VerificationResult::Passed;
        }

        if pre_text.is_empty() && post_text.is_empty() {
            return VerificationResult::Uncertain {
                reason: "no OCR data available in either snapshot".into(),
            };
        }

        VerificationResult::Failed {
            reason: "text content did not change after step execution".into(),
            suggestion: "verify the step performed an action that modifies text on screen".into(),
        }
    }

    /// Verify that a specific application is now the active foreground app.
    pub async fn verify_active_app_changed(&self, expected: &str) -> VerificationResult {
        let ws = self.world_state.read();
        match ws.active_app() {
            Some(app) if app == expected => VerificationResult::Passed,
            Some(app) => VerificationResult::Failed {
                reason: format!(
                    "expected active app '{}' but current active app is '{}'",
                    expected, app
                ),
                suggestion: "check if the app was launched successfully or if it crashed".into(),
            },
            None => VerificationResult::Uncertain {
                reason: "no active app information available in world state".into(),
            },
        }
    }

    /// Verify that the world state changed between pre- and post-execution
    /// snapshots.
    ///
    /// Uses a field-level comparison of the two snapshots.
    pub async fn verify_world_state_diff(
        &self,
        pre_snapshot: &WorldSnapshot,
    ) -> VerificationResult {
        let ws = self.world_state.read();
        let current = ws.snapshot();
        drop(ws);

        let diff = compare_snapshots(pre_snapshot, &current);

        if diff.has_any_change {
            VerificationResult::Passed
        } else {
            VerificationResult::Failed {
                reason: "no world state change detected after step execution".into(),
                suggestion: concat!(
                    "verify the step action type matches the expected outcome; ",
                    "the step may have been a no-op"
                )
                .into(),
            }
        }
    }

    /// Verify a device telemetry field matches an expected value.
    ///
    /// Supports fields: `wifi`, `bluetooth` (expected `"enabled"` or
    /// `"disabled"`), and `battery` (expected as number string).
    ///
    /// Returns `Uncertain` if device telemetry is not available or the field
    /// is not supported.
    pub async fn verify_device_state(
        &self,
        field: &str,
        expected: &str,
        _pre_snapshot: Option<&WorldSnapshot>,
    ) -> VerificationResult {
        let ws = self.world_state.read();
        let telemetry = match ws.device_telemetry() {
            Some(t) => t.clone(),
            None => {
                return VerificationResult::Uncertain {
                    reason: "no device telemetry available in world state".into(),
                }
            }
        };
        drop(ws);

        match field {
            "wifi" => check_bool_field(telemetry.wifi_enabled, expected, "wifi"),
            "bluetooth" => check_bool_field(telemetry.bluetooth_enabled, expected, "bluetooth"),
            "battery" | "brightness" => check_optional_u8(telemetry.battery_level, expected, field),
            _ => VerificationResult::Uncertain {
                reason: format!(
                    "device telemetry field '{}' is not yet supported for verification",
                    field
                ),
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Compare two [`WorldSnapshot`] values field-by-field and produce a
/// [`WorldDiff`].
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

/// Check a boolean telemetry field against an expected string
/// (`"enabled"` / `"disabled"`).
fn check_bool_field(value: Option<bool>, expected: &str, field_name: &str) -> VerificationResult {
    match value {
        Some(actual) => {
            let actual_str = if actual { "enabled" } else { "disabled" };
            if actual_str == expected {
                VerificationResult::Passed
            } else {
                VerificationResult::Failed {
                    reason: format!(
                        "expected {} = '{}', actual = '{}'",
                        field_name, expected, actual_str
                    ),
                    suggestion: "check device control action parameters".into(),
                }
            }
        }
        None => VerificationResult::Uncertain {
            reason: format!("'{}' state not available in device telemetry", field_name),
        },
    }
}

/// Check an optional `u8` telemetry field against an expected number string.
fn check_optional_u8(value: Option<u8>, expected: &str, field_name: &str) -> VerificationResult {
    match value {
        Some(actual) => {
            let actual_str = actual.to_string();
            if actual_str == expected {
                VerificationResult::Passed
            } else {
                VerificationResult::Failed {
                    reason: format!(
                        "expected {} = '{}', actual = '{}'",
                        field_name, expected, actual_str
                    ),
                    suggestion: "check device control action parameters".into(),
                }
            }
        }
        None => VerificationResult::Uncertain {
            reason: format!("'{}' value not available in device telemetry", field_name),
        },
    }
}

/// Capture a frame with a timeout.
#[allow(clippy::await_holding_lock)]
async fn capture_frame_with_timeout(
    screen: &Arc<RwLock<ScreenEngine>>,
    timeout: Duration,
) -> Result<CapturedFrame, String> {
    tokio::time::timeout(timeout, async {
        let mut engine = screen.write();
        engine.capture_frame().await
    })
    .await
    .map_err(|_| "frame capture timed out".to_string())?
    .map_err(|e| format!("{}", e))
}

/// Run OCR on a captured frame with a timeout.
#[allow(clippy::await_holding_lock)]
async fn ocr_frame_with_timeout(
    screen: &Arc<RwLock<ScreenEngine>>,
    frame: &CapturedFrame,
    timeout: Duration,
) -> Result<OCRResult, String> {
    tokio::time::timeout(timeout, async {
        let engine = screen.read();
        engine.recognize_text(frame).await
    })
    .await
    .map_err(|_| "OCR timed out".to_string())?
    .map_err(|e| format!("{}", e))
}

/// Ground an element on a captured frame with a timeout.
#[allow(clippy::await_holding_lock)]
async fn ground_element_with_timeout(
    screen: &Arc<RwLock<ScreenEngine>>,
    frame: &CapturedFrame,
    query: &GroundingQuery,
    timeout: Duration,
) -> Result<GroundingResult, String> {
    tokio::time::timeout(timeout, async {
        let engine = screen.read();
        engine.ground_element(frame, query).await
    })
    .await
    .map_err(|_| "element grounding timed out".to_string())?
    .map_err(|e| format!("{}", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::{ActionType, DeviceControl};
    use crate::pipeline_step::{ExpectedOutcome, PipelineStep, RetryPolicy};
    use crate::planner::ExecutionStep;
    use crate::world_state::{DeviceTelemetry, WorldState};
    use nova_screen::{OCRRegion, PixelFormat, Rect};

    // -----------------------------------------------------------------------
    // Test helpers
    // -----------------------------------------------------------------------

    fn now_millis() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0)
    }

    fn make_world_state() -> WorldState {
        WorldState::new()
    }

    fn make_dummy_step(action: ActionType) -> PipelineStep {
        let step = ExecutionStep {
            id: "test_step".to_string(),
            description: "test step".to_string(),
            action,
            dependencies: vec![],
            required_capabilities: vec![],
            timeout_ms: 5000,
            retry_count: 0,
            continue_on_failure: false,
        };
        PipelineStep::new(
            step,
            0,
            vec![],
            VerificationStrategy::NoVerification,
            ExpectedOutcome::NoChange,
            RetryPolicy::NoRetry,
        )
    }

    fn dummy_frame() -> CapturedFrame {
        CapturedFrame {
            frame_id: "frame_test".into(),
            timestamp: now_millis(),
            width: 1920,
            height: 1080,
            format: PixelFormat::RGBA8,
            data: vec![0u8; 64],
            region: None,
        }
    }

    fn dummy_ocr_result(text: &str) -> OCRResult {
        OCRResult {
            text: text.into(),
            confidence: 0.95,
            language: "en".into(),
            regions: vec![OCRRegion {
                text: text.into(),
                confidence: 0.95,
                bounds: Rect {
                    x: 0,
                    y: 0,
                    width: 100,
                    height: 20,
                },
            }],
        }
    }

    fn success_result() -> ActionResult {
        ActionResult::success("ok")
    }

    // -----------------------------------------------------------------------
    // Constructor tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_outcome_verifier_new() {
        let ws = Arc::new(RwLock::new(make_world_state()));
        let verifier = OutcomeVerifier::new(ws.clone(), None);
        assert!(verifier.screen.is_none());
        assert_eq!(verifier.default_timeout, Duration::from_secs(10));
    }

    #[test]
    fn test_outcome_verifier_with_timeout() {
        let ws = Arc::new(RwLock::new(make_world_state()));
        let verifier =
            OutcomeVerifier::new(ws.clone(), None).with_default_timeout(Duration::from_secs(30));
        assert_eq!(verifier.default_timeout, Duration::from_secs(30));
    }

    // -----------------------------------------------------------------------
    // VerificationResult tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_verification_result_is_passed() {
        assert!(VerificationResult::Passed.is_passed());
        assert!(!VerificationResult::Failed {
            reason: "".into(),
            suggestion: "".into(),
        }
        .is_passed());
        assert!(!VerificationResult::Uncertain { reason: "".into() }.is_passed());
    }

    #[test]
    fn test_verification_result_is_failed() {
        assert!(!VerificationResult::Passed.is_failed());
        assert!(VerificationResult::Failed {
            reason: "err".into(),
            suggestion: "fix".into(),
        }
        .is_failed());
        assert!(!VerificationResult::Uncertain { reason: "".into() }.is_failed());
    }

    #[test]
    fn test_verification_result_is_uncertain() {
        assert!(!VerificationResult::Passed.is_uncertain());
        assert!(!VerificationResult::Failed {
            reason: "".into(),
            suggestion: "".into(),
        }
        .is_uncertain());
        assert!(VerificationResult::Uncertain { reason: "?".into() }.is_uncertain());
    }

    #[test]
    fn test_verification_result_summary() {
        assert_eq!(VerificationResult::Passed.summary(), "PASSED");
        assert!(VerificationResult::Failed {
            reason: "broken".into(),
            suggestion: "fix it".into(),
        }
        .summary()
        .contains("broken"));
        assert!(VerificationResult::Uncertain {
            reason: "no data".into()
        }
        .summary()
        .contains("no data"));
    }

    // -----------------------------------------------------------------------
    // VerificationEvidence tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_verification_evidence_default() {
        let evidence = VerificationEvidence::new();
        assert!(!evidence.has_any());
        assert!(evidence.pre_snapshot.is_none());
        assert!(evidence.post_snapshot.is_none());
        assert!(evidence.world_diff.is_none());
        assert!(evidence.captured_frame.is_none());
        assert!(evidence.ocr_result.is_none());
        assert!(evidence.grounded_elements.is_empty());
    }

    #[test]
    fn test_verification_evidence_has_any() {
        let evidence = VerificationEvidence {
            pre_snapshot: Some(WorldSnapshot {
                frame: None,
                active_app: None,
                ocr: None,
                grounded_elements: vec![],
                ui_tree: None,
                device_telemetry: None,
                network_state: None,
                timestamp: 0,
            }),
            ..Default::default()
        };
        assert!(evidence.has_any());
    }

    // -----------------------------------------------------------------------
    // NoVerification tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_verify_no_verification() {
        let ws = Arc::new(RwLock::new(make_world_state()));
        let verifier = OutcomeVerifier::new(ws, None);
        let step = make_dummy_step(ActionType::Wait { duration_ms: 1 });

        let (result, evidence) = verifier.verify(&step, &success_result(), None).await;
        assert!(result.is_passed());
        assert!(evidence.post_snapshot.is_some());
    }

    // -----------------------------------------------------------------------
    // Action execution failure
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_verify_action_failed_returns_failed_immediately() {
        let ws = Arc::new(RwLock::new(make_world_state()));
        let verifier = OutcomeVerifier::new(ws, None);
        let step = make_dummy_step(ActionType::Wait { duration_ms: 1 });
        let failed = ActionResult::failure("something went wrong");

        let (result, evidence) = verifier.verify(&step, &failed, None).await;
        assert!(result.is_failed());
        if let VerificationResult::Failed { reason, .. } = &result {
            assert!(reason.contains("something went wrong"));
        }
        assert!(evidence.pre_snapshot.is_none());
    }

    // -----------------------------------------------------------------------
    // AppInForeground tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_verify_app_in_foreground_matching() {
        let ws = Arc::new(RwLock::new(make_world_state()));
        ws.write().update_active_app("calculator".into());
        let verifier = OutcomeVerifier::new(ws, None);
        let step = PipelineStep::new(
            ExecutionStep {
                id: "s1".into(),
                description: "open calc".into(),
                action: ActionType::OpenApp {
                    app_id: "calculator".into(),
                    data: None,
                },
                dependencies: vec![],
                required_capabilities: vec![],
                timeout_ms: 5000,
                retry_count: 0,
                continue_on_failure: false,
            },
            0,
            vec![],
            VerificationStrategy::AppInForeground {
                app_name: "calculator".into(),
            },
            ExpectedOutcome::AppForeground {
                app_name: "calculator".into(),
            },
            RetryPolicy::NoRetry,
        );

        let (result, _) = verifier.verify(&step, &success_result(), None).await;
        assert!(result.is_passed());
    }

    #[tokio::test]
    async fn test_verify_app_in_foreground_not_matching() {
        let ws = Arc::new(RwLock::new(make_world_state()));
        ws.write().update_active_app("notepad".into());
        let verifier = OutcomeVerifier::new(ws, None);
        let step = PipelineStep::new(
            ExecutionStep {
                id: "s1".into(),
                description: "open calc".into(),
                action: ActionType::OpenApp {
                    app_id: "calculator".into(),
                    data: None,
                },
                dependencies: vec![],
                required_capabilities: vec![],
                timeout_ms: 5000,
                retry_count: 0,
                continue_on_failure: false,
            },
            0,
            vec![],
            VerificationStrategy::AppInForeground {
                app_name: "calculator".into(),
            },
            ExpectedOutcome::AppForeground {
                app_name: "calculator".into(),
            },
            RetryPolicy::NoRetry,
        );

        let (result, _) = verifier.verify(&step, &success_result(), None).await;
        assert!(result.is_failed());
        if let VerificationResult::Failed { reason, .. } = &result {
            assert!(reason.contains("notepad"));
        }
    }

    #[tokio::test]
    async fn test_verify_app_in_foreground_no_active_app() {
        let ws = Arc::new(RwLock::new(make_world_state()));
        let verifier = OutcomeVerifier::new(ws, None);
        let step = PipelineStep::new(
            ExecutionStep {
                id: "s1".into(),
                description: "open calc".into(),
                action: ActionType::OpenApp {
                    app_id: "calculator".into(),
                    data: None,
                },
                dependencies: vec![],
                required_capabilities: vec![],
                timeout_ms: 5000,
                retry_count: 0,
                continue_on_failure: false,
            },
            0,
            vec![],
            VerificationStrategy::AppInForeground {
                app_name: "calculator".into(),
            },
            ExpectedOutcome::AppForeground {
                app_name: "calculator".into(),
            },
            RetryPolicy::NoRetry,
        );

        let (result, _) = verifier.verify(&step, &success_result(), None).await;
        assert!(result.is_uncertain());
    }

    // -----------------------------------------------------------------------
    // DeviceTelemetryMatch tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_verify_device_telemetry_wifi_match() {
        let ws = Arc::new(RwLock::new(make_world_state()));
        ws.write().update_device_telemetry(DeviceTelemetry {
            wifi_enabled: Some(true),
            ..DeviceTelemetry::new()
        });
        let verifier = OutcomeVerifier::new(ws, None);
        let step = PipelineStep::new(
            ExecutionStep {
                id: "s1".into(),
                description: "enable wifi".into(),
                action: ActionType::DeviceControl {
                    control: DeviceControl::ToggleWiFi(true),
                },
                dependencies: vec![],
                required_capabilities: vec![],
                timeout_ms: 5000,
                retry_count: 0,
                continue_on_failure: false,
            },
            0,
            vec![],
            VerificationStrategy::DeviceTelemetryMatch {
                field: "wifi".into(),
                expected: "enabled".into(),
            },
            ExpectedOutcome::DeviceStateChange {
                field: "wifi".into(),
            },
            RetryPolicy::NoRetry,
        );

        let (result, _) = verifier.verify(&step, &success_result(), None).await;
        assert!(result.is_passed());
    }

    #[tokio::test]
    async fn test_verify_device_telemetry_wifi_mismatch() {
        let ws = Arc::new(RwLock::new(make_world_state()));
        ws.write().update_device_telemetry(DeviceTelemetry {
            wifi_enabled: Some(false),
            ..DeviceTelemetry::new()
        });
        let verifier = OutcomeVerifier::new(ws, None);
        let step = PipelineStep::new(
            ExecutionStep {
                id: "s1".into(),
                description: "disable wifi".into(),
                action: ActionType::DeviceControl {
                    control: DeviceControl::ToggleWiFi(false),
                },
                dependencies: vec![],
                required_capabilities: vec![],
                timeout_ms: 5000,
                retry_count: 0,
                continue_on_failure: false,
            },
            0,
            vec![],
            VerificationStrategy::DeviceTelemetryMatch {
                field: "wifi".into(),
                expected: "disabled".into(),
            },
            ExpectedOutcome::DeviceStateChange {
                field: "wifi".into(),
            },
            RetryPolicy::NoRetry,
        );

        // Set to disabled
        let (result, _) = verifier.verify(&step, &success_result(), None).await;
        assert!(result.is_passed());

        // Now check mismatch: expected enabled but is disabled
        let step2 = PipelineStep::new(
            ExecutionStep {
                id: "s1".into(),
                description: "enable wifi".into(),
                action: ActionType::DeviceControl {
                    control: DeviceControl::ToggleWiFi(true),
                },
                dependencies: vec![],
                required_capabilities: vec![],
                timeout_ms: 5000,
                retry_count: 0,
                continue_on_failure: false,
            },
            0,
            vec![],
            VerificationStrategy::DeviceTelemetryMatch {
                field: "wifi".into(),
                expected: "enabled".into(),
            },
            ExpectedOutcome::DeviceStateChange {
                field: "wifi".into(),
            },
            RetryPolicy::NoRetry,
        );

        let (result, _) = verifier.verify(&step2, &success_result(), None).await;
        assert!(result.is_failed());
    }

    #[tokio::test]
    async fn test_verify_device_telemetry_no_telemetry() {
        let ws = Arc::new(RwLock::new(make_world_state()));
        let verifier = OutcomeVerifier::new(ws, None);
        let step = PipelineStep::new(
            ExecutionStep {
                id: "s1".into(),
                description: "check wifi".into(),
                action: ActionType::DeviceControl {
                    control: DeviceControl::ToggleWiFi(true),
                },
                dependencies: vec![],
                required_capabilities: vec![],
                timeout_ms: 5000,
                retry_count: 0,
                continue_on_failure: false,
            },
            0,
            vec![],
            VerificationStrategy::DeviceTelemetryMatch {
                field: "wifi".into(),
                expected: "enabled".into(),
            },
            ExpectedOutcome::DeviceStateChange {
                field: "wifi".into(),
            },
            RetryPolicy::NoRetry,
        );

        let (result, _) = verifier.verify(&step, &success_result(), None).await;
        assert!(result.is_uncertain());
    }

    #[tokio::test]
    async fn test_verify_device_telemetry_bluetooth() {
        let ws = Arc::new(RwLock::new(make_world_state()));
        ws.write().update_device_telemetry(DeviceTelemetry {
            bluetooth_enabled: Some(true),
            ..DeviceTelemetry::new()
        });
        let verifier = OutcomeVerifier::new(ws, None);
        let step = PipelineStep::new(
            ExecutionStep {
                id: "s1".into(),
                description: "enable bt".into(),
                action: ActionType::DeviceControl {
                    control: DeviceControl::ToggleBluetooth(true),
                },
                dependencies: vec![],
                required_capabilities: vec![],
                timeout_ms: 5000,
                retry_count: 0,
                continue_on_failure: false,
            },
            0,
            vec![],
            VerificationStrategy::DeviceTelemetryMatch {
                field: "bluetooth".into(),
                expected: "enabled".into(),
            },
            ExpectedOutcome::DeviceStateChange {
                field: "bluetooth".into(),
            },
            RetryPolicy::NoRetry,
        );

        let (result, _) = verifier.verify(&step, &success_result(), None).await;
        assert!(result.is_passed());
    }

    // -----------------------------------------------------------------------
    // CompareSnapshots tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_verify_compare_snapshots_no_pre_snapshot() {
        let ws = Arc::new(RwLock::new(make_world_state()));
        let verifier = OutcomeVerifier::new(ws, None);
        let step = PipelineStep::new(
            ExecutionStep {
                id: "s1".into(),
                description: "drag element".into(),
                action: ActionType::DragScreenElements {
                    from_query: "slider".into(),
                    to_query: "pos".into(),
                },
                dependencies: vec![],
                required_capabilities: vec![],
                timeout_ms: 5000,
                retry_count: 0,
                continue_on_failure: false,
            },
            0,
            vec![],
            VerificationStrategy::CompareSnapshots,
            ExpectedOutcome::ScreenChange {
                description: "screen after drag".into(),
            },
            RetryPolicy::NoRetry,
        );

        let (result, _) = verifier.verify(&step, &success_result(), None).await;
        assert!(result.is_uncertain());
    }

    #[tokio::test]
    async fn test_verify_compare_snapshots_with_change() {
        let ws = Arc::new(RwLock::new(make_world_state()));
        let verifier = OutcomeVerifier::new(ws.clone(), None);

        // Create pre-snapshot with one active app
        let pre = WorldSnapshot {
            frame: Some(dummy_frame()),
            active_app: Some("old_app".into()),
            ocr: None,
            grounded_elements: vec![],
            ui_tree: None,
            device_telemetry: None,
            network_state: None,
            timestamp: 1000,
        };

        // Update world state to have a different active app
        ws.write().update_active_app("new_app".into());

        let step = make_dummy_step(ActionType::Wait { duration_ms: 1 });
        // Set verification strategy to CompareSnapshots
        let mut step2 = step;
        step2.verification = VerificationStrategy::CompareSnapshots;

        let (result, evidence) = verifier.verify(&step2, &success_result(), Some(&pre)).await;
        assert!(result.is_passed());
        assert!(evidence.world_diff.is_some());
        assert!(evidence.world_diff.as_ref().unwrap().active_app_changed);
    }

    #[tokio::test]
    async fn test_verify_compare_snapshots_no_change() {
        let ws = Arc::new(RwLock::new(make_world_state()));
        let verifier = OutcomeVerifier::new(ws, None);

        // Pre-snapshot matches the empty world state exactly
        let pre = WorldSnapshot {
            frame: None,
            active_app: None,
            ocr: None,
            grounded_elements: vec![],
            ui_tree: None,
            device_telemetry: None,
            network_state: None,
            timestamp: 1000,
        };

        // No changes to world state (both pre and current are empty)
        let step = make_dummy_step(ActionType::Wait { duration_ms: 1 });
        let mut step2 = step;
        step2.verification = VerificationStrategy::CompareSnapshots;

        let (result, _) = verifier.verify(&step2, &success_result(), Some(&pre)).await;
        // Since both have no changes, this should fail (no world state change detected)
        assert!(result.is_failed());
    }

    // -----------------------------------------------------------------------
    // Screen-based verification (no screen engine available)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_verify_screen_contains_no_screen() {
        let ws = Arc::new(RwLock::new(make_world_state()));
        let verifier = OutcomeVerifier::new(ws, None);

        let result = verifier.verify_screen_contains("hello").await;
        assert!(result.is_uncertain());
        if let VerificationResult::Uncertain { reason } = &result {
            assert!(reason.contains("not available"));
        }
    }

    #[tokio::test]
    async fn test_verify_element_exists_no_screen() {
        let ws = Arc::new(RwLock::new(make_world_state()));
        let verifier = OutcomeVerifier::new(ws, None);

        let result = verifier.verify_element_exists("button").await;
        assert!(result.is_uncertain());
        if let VerificationResult::Uncertain { reason } = &result {
            assert!(reason.contains("not available"));
        }
    }

    // -----------------------------------------------------------------------
    // verify_text_changed tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_verify_text_changed_no_ocr_data() {
        let ws = Arc::new(RwLock::new(make_world_state()));
        let verifier = OutcomeVerifier::new(ws, None);

        let pre = WorldSnapshot {
            frame: None,
            active_app: None,
            ocr: None,
            grounded_elements: vec![],
            ui_tree: None,
            device_telemetry: None,
            network_state: None,
            timestamp: 0,
        };

        let result = verifier.verify_text_changed(&pre).await;
        assert!(result.is_uncertain());
    }

    #[tokio::test]
    async fn test_verify_text_changed_with_ocr_change() {
        let ws = Arc::new(RwLock::new(make_world_state()));
        ws.write().update_ocr(dummy_ocr_result("new text content"));
        let verifier = OutcomeVerifier::new(ws, None);

        let pre = WorldSnapshot {
            frame: None,
            active_app: None,
            ocr: Some(dummy_ocr_result("old text")),
            grounded_elements: vec![],
            ui_tree: None,
            device_telemetry: None,
            network_state: None,
            timestamp: 0,
        };

        let result = verifier.verify_text_changed(&pre).await;
        assert!(result.is_passed());
    }

    #[tokio::test]
    async fn test_verify_text_changed_no_actual_change() {
        let ws = Arc::new(RwLock::new(make_world_state()));
        ws.write().update_ocr(dummy_ocr_result("same text"));
        let verifier = OutcomeVerifier::new(ws, None);

        let pre = WorldSnapshot {
            frame: None,
            active_app: None,
            ocr: Some(dummy_ocr_result("same text")),
            grounded_elements: vec![],
            ui_tree: None,
            device_telemetry: None,
            network_state: None,
            timestamp: 0,
        };

        let result = verifier.verify_text_changed(&pre).await;
        assert!(result.is_failed());
    }

    // -----------------------------------------------------------------------
    // verify_world_state_diff tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_verify_world_state_diff_detects_change() {
        let ws = Arc::new(RwLock::new(make_world_state()));
        ws.write().update_active_app("new_app".into());
        let verifier = OutcomeVerifier::new(ws, None);

        let pre = WorldSnapshot {
            frame: None,
            active_app: Some("old_app".into()),
            ocr: None,
            grounded_elements: vec![],
            ui_tree: None,
            device_telemetry: None,
            network_state: None,
            timestamp: 0,
        };

        let result = verifier.verify_world_state_diff(&pre).await;
        assert!(result.is_passed());
    }

    #[tokio::test]
    async fn test_verify_world_state_diff_no_change() {
        let ws = Arc::new(RwLock::new(make_world_state()));
        ws.write().update_active_app("same_app".into());
        let verifier = OutcomeVerifier::new(ws, None);

        let pre = WorldSnapshot {
            frame: None,
            active_app: Some("same_app".into()),
            ocr: None,
            grounded_elements: vec![],
            ui_tree: None,
            device_telemetry: None,
            network_state: None,
            timestamp: 0,
        };

        let result = verifier.verify_world_state_diff(&pre).await;
        assert!(result.is_failed());
    }

    // -----------------------------------------------------------------------
    // snapshots with evidence collection
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_verify_collects_evidence() {
        let ws = Arc::new(RwLock::new(make_world_state()));
        ws.write().update_active_app("test_app".into());
        let verifier = OutcomeVerifier::new(ws, None);

        let step = make_dummy_step(ActionType::Wait { duration_ms: 1 });

        let (_, evidence) = verifier.verify(&step, &success_result(), None).await;
        assert!(evidence.post_snapshot.is_some());
        assert_eq!(
            evidence.post_snapshot.unwrap().active_app,
            Some("test_app".into())
        );
    }

    // -----------------------------------------------------------------------
    // compare_snapshots helper tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_compare_snapshots_detects_active_app_change() {
        let pre = WorldSnapshot {
            frame: None,
            active_app: Some("app1".into()),
            ocr: None,
            grounded_elements: vec![],
            ui_tree: None,
            device_telemetry: None,
            network_state: None,
            timestamp: 0,
        };
        let post = WorldSnapshot {
            frame: None,
            active_app: Some("app2".into()),
            ocr: None,
            grounded_elements: vec![],
            ui_tree: None,
            device_telemetry: None,
            network_state: None,
            timestamp: 1,
        };

        let diff = compare_snapshots(&pre, &post);
        assert!(diff.active_app_changed);
        assert!(diff.has_any_change);
        assert!(!diff.frame_changed);
    }

    #[test]
    fn test_compare_snapshots_no_changes() {
        let pre = WorldSnapshot {
            frame: None,
            active_app: None,
            ocr: None,
            grounded_elements: vec![],
            ui_tree: None,
            device_telemetry: None,
            network_state: None,
            timestamp: 0,
        };
        let post = WorldSnapshot {
            frame: None,
            active_app: None,
            ocr: None,
            grounded_elements: vec![],
            ui_tree: None,
            device_telemetry: None,
            network_state: None,
            timestamp: 1,
        };

        let diff = compare_snapshots(&pre, &post);
        assert!(!diff.has_any_change);
    }

    #[test]
    fn test_compare_snapshots_detects_device_change() {
        let pre = WorldSnapshot {
            frame: None,
            active_app: None,
            ocr: None,
            grounded_elements: vec![],
            ui_tree: None,
            device_telemetry: Some(DeviceTelemetry {
                battery_level: Some(80),
                ..DeviceTelemetry::new()
            }),
            network_state: None,
            timestamp: 0,
        };
        let post = WorldSnapshot {
            frame: None,
            active_app: None,
            ocr: None,
            grounded_elements: vec![],
            ui_tree: None,
            device_telemetry: Some(DeviceTelemetry {
                battery_level: Some(50),
                ..DeviceTelemetry::new()
            }),
            network_state: None,
            timestamp: 1,
        };

        let diff = compare_snapshots(&pre, &post);
        assert!(diff.device_state_changed);
        assert!(diff.has_any_change);
    }

    // -----------------------------------------------------------------------
    // verify_device_state direct tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_verify_device_state_unsupported_field() {
        let ws = Arc::new(RwLock::new(make_world_state()));
        ws.write().update_device_telemetry(DeviceTelemetry {
            battery_level: Some(50),
            ..DeviceTelemetry::new()
        });
        let verifier = OutcomeVerifier::new(ws, None);

        let result = verifier.verify_device_state("volume", "50", None).await;
        assert!(result.is_uncertain());
    }

    // -----------------------------------------------------------------------
    // verify_active_app_changed direct tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_verify_active_app_changed_direct_matching() {
        let ws = Arc::new(RwLock::new(make_world_state()));
        ws.write().update_active_app("notepad".into());
        let verifier = OutcomeVerifier::new(ws, None);

        let result = verifier.verify_active_app_changed("notepad").await;
        assert!(result.is_passed());
    }

    #[tokio::test]
    async fn test_verify_active_app_changed_direct_not_matching() {
        let ws = Arc::new(RwLock::new(make_world_state()));
        ws.write().update_active_app("calc".into());
        let verifier = OutcomeVerifier::new(ws, None);

        let result = verifier.verify_active_app_changed("notepad").await;
        assert!(result.is_failed());
    }
}
