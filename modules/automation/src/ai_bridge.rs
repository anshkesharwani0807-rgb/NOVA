use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use crate::execution_manager::{ExecutionManager, ExecutionPriority, ExecutionRequest};
use crate::goal_registry::GoalRegistry;
use crate::intention_parser::IntentParser;
use crate::planner::Planner;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AutomationIntent {
    Execute,
    Query,
    Clarify,
    Confirm,
    Cancel,
    Status,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AutomationDecision {
    Execute {
        execution_id: String,
        goal: String,
    },
    Clarify {
        question: String,
        options: Vec<String>,
    },
    Confirm {
        goal: String,
        stakes: String,
    },
    Query {
        answer: String,
    },
    Status {
        execution_id: String,
        status: String,
        progress: f32,
    },
    Cancelled {
        execution_id: String,
    },
    Error {
        message: String,
    },
    Unknown {
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AutomationCapability {
    NaturalLanguage,
    MultiStep,
    Cancellation,
    Clarification,
    ContextAware,
    PriorityScheduling,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationRequest {
    pub text: String,
    pub session_id: Option<String>,
    pub context: Option<AutomationContext>,
    pub priority: Option<ExecutionPriority>,
}

impl AutomationRequest {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            session_id: None,
            context: None,
            priority: None,
        }
    }

    pub fn with_session(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    pub fn with_context(mut self, ctx: AutomationContext) -> Self {
        self.context = Some(ctx);
        self
    }

    pub fn with_priority(mut self, priority: ExecutionPriority) -> Self {
        self.priority = Some(priority);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationResponse {
    pub success: bool,
    pub decision: AutomationDecision,
    pub execution_id: Option<String>,
    pub message: String,
    pub suggestions: Vec<String>,
}

impl AutomationResponse {
    pub fn success(decision: AutomationDecision, message: impl Into<String>) -> Self {
        Self {
            success: true,
            decision,
            execution_id: None,
            message: message.into(),
            suggestions: Vec::new(),
        }
    }

    pub fn error(msg: impl Into<String>) -> Self {
        let msg = msg.into();
        Self {
            success: false,
            decision: AutomationDecision::Error {
                message: msg.clone(),
            },
            execution_id: None,
            message: msg,
            suggestions: Vec::new(),
        }
    }

    pub fn with_execution_id(mut self, id: impl Into<String>) -> Self {
        self.execution_id = Some(id.into());
        self
    }

    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestions.push(suggestion.into());
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationContext {
    pub active_goal: Option<String>,
    pub last_intent: Option<String>,
    pub execution_count: u64,
    pub previous_goals: Vec<String>,
    pub user_preferences: HashMap<String, String>,
}

impl AutomationContext {
    pub fn new() -> Self {
        Self {
            active_goal: None,
            last_intent: None,
            execution_count: 0,
            previous_goals: Vec::new(),
            user_preferences: HashMap::new(),
        }
    }
}

impl Default for AutomationContext {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct AutomationSession {
    pub id: String,
    pub history: Vec<SessionEntry>,
    pub context: AutomationContext,
    pub active_executions: HashMap<String, AutomationDecision>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone)]
pub struct SessionEntry {
    pub request: String,
    pub response: AutomationResponse,
    pub timestamp: i64,
}

impl AutomationSession {
    pub fn new(id: impl Into<String>) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        Self {
            id: id.into(),
            history: Vec::new(),
            context: AutomationContext::new(),
            active_executions: HashMap::new(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn add_entry(&mut self, request: String, response: AutomationResponse) {
        let now = chrono::Utc::now().timestamp_millis();
        self.history.push(SessionEntry {
            request,
            response: response.clone(),
            timestamp: now,
        });
        self.updated_at = now;
        self.context.execution_count += 1;
        if let AutomationDecision::Execute {
            execution_id: _,
            ref goal,
        } = response.decision
        {
            self.context.previous_goals.push(goal.clone());
            self.context.last_intent = Some(goal.clone());
        }
    }

    pub fn clear_history(&mut self) {
        self.history.clear();
        self.context = AutomationContext::new();
        self.updated_at = chrono::Utc::now().timestamp_millis();
    }
}

#[derive(Debug, Clone)]
pub struct AutomationBridgeConfig {
    pub default_session_ttl_ms: i64,
    pub max_session_history: usize,
    pub enable_clarification: bool,
    pub auto_execute_high_confidence: bool,
    pub minimum_confidence_threshold: f32,
}

impl Default for AutomationBridgeConfig {
    fn default() -> Self {
        Self {
            default_session_ttl_ms: 600_000,
            max_session_history: 50,
            enable_clarification: true,
            auto_execute_high_confidence: true,
            minimum_confidence_threshold: 0.7,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct AutomationMetrics {
    pub total_requests: u64,
    pub successful_executions: u64,
    pub failed_executions: u64,
    pub clarifications_requested: u64,
    pub cancelled_requests: u64,
    pub average_processing_time_ms: u64,
    pub peak_sessions: u64,
}

impl AutomationMetrics {
    fn new() -> Self {
        Self::default()
    }

    fn record_request(&mut self) {
        self.total_requests += 1;
    }

    fn record_success(&mut self) {
        self.successful_executions += 1;
    }

    fn record_failure(&mut self) {
        self.failed_executions += 1;
    }

    fn record_clarification(&mut self) {
        self.clarifications_requested += 1;
    }

    fn record_cancellation(&mut self) {
        self.cancelled_requests += 1;
    }
}

#[derive(Debug, Clone)]
pub enum BridgeError {
    InvalidRequest(String),
    UnknownIntent(String),
    ExecutionFailed(String),
    SessionNotFound(String),
    ContextExpired(String),
    Internal(String),
}

impl std::fmt::Display for BridgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BridgeError::InvalidRequest(msg) => write!(f, "invalid request: {}", msg),
            BridgeError::UnknownIntent(msg) => write!(f, "unknown intent: {}", msg),
            BridgeError::ExecutionFailed(msg) => write!(f, "execution failed: {}", msg),
            BridgeError::SessionNotFound(id) => write!(f, "session not found: {}", id),
            BridgeError::ContextExpired(msg) => write!(f, "context expired: {}", msg),
            BridgeError::Internal(msg) => write!(f, "internal error: {}", msg),
        }
    }
}

pub struct AIAutomationBridge {
    #[allow(dead_code)]
    ai_engine: Arc<nova_ai::AIEngine>,
    intent_parser: Arc<IntentParser>,
    goal_registry: Arc<GoalRegistry>,
    planner: Arc<Planner>,
    execution_manager: Arc<ExecutionManager>,
    sessions: Arc<RwLock<HashMap<String, Arc<RwLock<AutomationSession>>>>>,
    config: Arc<RwLock<AutomationBridgeConfig>>,
    metrics: Arc<RwLock<AutomationMetrics>>,
}

impl AIAutomationBridge {
    pub fn new(
        ai_engine: Arc<nova_ai::AIEngine>,
        intent_parser: Arc<IntentParser>,
        goal_registry: Arc<GoalRegistry>,
        planner: Arc<Planner>,
        execution_manager: Arc<ExecutionManager>,
    ) -> Self {
        Self {
            ai_engine,
            intent_parser,
            goal_registry,
            planner,
            execution_manager,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            config: Arc::new(RwLock::new(AutomationBridgeConfig::default())),
            metrics: Arc::new(RwLock::new(AutomationMetrics::new())),
        }
    }

    pub fn with_config(self, config: AutomationBridgeConfig) -> Self {
        *self.config.write() = config;
        self
    }

    pub fn config(&self) -> AutomationBridgeConfig {
        self.config.read().clone()
    }

    pub fn set_config(&self, config: AutomationBridgeConfig) {
        *self.config.write() = config;
    }

    pub fn execute(&self, request: AutomationRequest) -> Result<AutomationResponse, BridgeError> {
        let start = std::time::Instant::now();
        self.metrics.write().record_request();

        let session_id = request
            .session_id
            .clone()
            .unwrap_or_else(|| "default".to_string());

        let session = self.get_or_create_session(&session_id);

        if let Some(ref ctx) = request.context {
            session.write().context = ctx.clone();
        }

        let text = request.text.trim().to_string();
        if text.is_empty() {
            return Err(BridgeError::InvalidRequest(
                "request text cannot be empty".into(),
            ));
        }

        let _priority = request.priority.unwrap_or(ExecutionPriority::Normal);

        let parse_result = self.intent_parser.parse(&text);
        if parse_result.is_unknown() {
            match self.handle_unknown_intent(&text, &session_id) {
                Ok(response) => {
                    let elapsed = start.elapsed().as_millis() as u64;
                    self.metrics.write().record_clarification();
                    self.update_session(session, text, response.clone());
                    self.update_metrics(elapsed, false, true);
                    return Ok(response);
                }
                Err(e) => {
                    self.metrics.write().record_failure();
                    return Err(e);
                }
            }
        }

        let intents = parse_result.intents();
        if intents.is_empty() {
            self.metrics.write().record_clarification();
            let response = AutomationResponse::success(
                AutomationDecision::Clarify {
                    question: "I couldn't understand your request. Could you rephrase it?".into(),
                    options: vec![],
                },
                "unclear request",
            );
            self.update_session(session, text, response.clone());
            return Ok(response);
        }

        let intent = intents[0].clone();
        let resolution = self.goal_registry.resolve(&intent);
        let goal_match = resolution.into_match();

        let goal_match = match goal_match {
            Some(m) => m,
            None => {
                self.metrics.write().record_failure();
                let response = AutomationResponse::error(format!(
                    "could not find a matching goal for '{}'",
                    text
                ));
                self.update_session(session, text, response.clone());
                return Ok(response);
            }
        };

        let goal = goal_match
            .definition
            .planner_template
            .fill(&goal_match.matched_parameters);

        let plan = match self.planner.plan(&goal) {
            Ok(p) => p,
            Err(e) => {
                self.metrics.write().record_failure();
                let response = AutomationResponse::error(format!("planning failed: {}", e));
                self.update_session(session, text, response.clone());
                return Ok(response);
            }
        };

        let priority = request.priority.unwrap_or(ExecutionPriority::Normal);
        let exec_request =
            ExecutionRequest::new(format!("ai_{}", uuid::Uuid::new_v4()), goal.clone())
                .with_priority(priority)
                .with_intent(intent);

        match self.execution_manager.submit(exec_request) {
            Ok(handle) => {
                let execution_id = handle.id().to_string();
                self.metrics.write().record_success();
                let elapsed = start.elapsed().as_millis() as u64;
                self.update_metrics(elapsed, true, false);

                let response = AutomationResponse::success(
                    AutomationDecision::Execute {
                        execution_id: execution_id.clone(),
                        goal: goal.description.clone(),
                    },
                    format!(
                        "executing: {} ({} step(s))",
                        goal.description, plan.estimated_steps
                    ),
                )
                .with_execution_id(&execution_id);

                session.write().active_executions.insert(
                    execution_id.clone(),
                    AutomationDecision::Execute {
                        execution_id,
                        goal: goal.description.clone(),
                    },
                );

                self.update_session(session, text, response.clone());
                Ok(response)
            }
            Err(e) => {
                self.metrics.write().record_failure();
                let response = AutomationResponse::error(format!("submission failed: {}", e));
                self.update_session(session, text, response.clone());
                Ok(response)
            }
        }
    }

    pub fn execute_stream(
        &self,
        request: AutomationRequest,
    ) -> Result<Vec<AutomationResponse>, BridgeError> {
        let response = self.execute(request)?;
        Ok(vec![response])
    }

    pub fn cancel(&self, execution_id: &str) -> Result<AutomationResponse, BridgeError> {
        match self.execution_manager.cancel(execution_id) {
            Ok(()) => {
                self.metrics.write().record_cancellation();
                Ok(AutomationResponse::success(
                    AutomationDecision::Cancelled {
                        execution_id: execution_id.to_string(),
                    },
                    format!("cancelled execution {}", execution_id),
                ))
            }
            Err(e) => Ok(AutomationResponse::error(format!(
                "cancellation failed: {}",
                e
            ))),
        }
    }

    pub fn session(&self, session_id: &str) -> Option<AutomationSession> {
        self.sessions
            .read()
            .get(session_id)
            .map(|s| s.read().clone())
    }

    pub fn reset_context(&self, session_id: &str) -> Result<(), BridgeError> {
        let sessions = self.sessions.read();
        if let Some(session) = sessions.get(session_id) {
            session.write().clear_history();
            Ok(())
        } else {
            Err(BridgeError::SessionNotFound(session_id.to_string()))
        }
    }

    pub fn capabilities(&self) -> Vec<AutomationCapability> {
        vec![
            AutomationCapability::NaturalLanguage,
            AutomationCapability::MultiStep,
            AutomationCapability::Cancellation,
            AutomationCapability::Clarification,
            AutomationCapability::ContextAware,
            AutomationCapability::PriorityScheduling,
        ]
    }

    pub fn metrics(&self) -> AutomationMetrics {
        self.metrics.read().clone()
    }

    fn get_or_create_session(&self, id: &str) -> Arc<RwLock<AutomationSession>> {
        let mut sessions = self.sessions.write();
        if !sessions.contains_key(id) {
            sessions.insert(
                id.to_string(),
                Arc::new(RwLock::new(AutomationSession::new(id))),
            );
        }
        sessions.get(id).unwrap().clone()
    }

    fn update_session(
        &self,
        session: Arc<RwLock<AutomationSession>>,
        request: String,
        response: AutomationResponse,
    ) {
        let mut s = session.write();
        s.add_entry(request, response);
    }

    fn update_metrics(&self, elapsed_ms: u64, _success: bool, clarification: bool) {
        let mut metrics = self.metrics.write();
        if clarification {
            metrics.clarifications_requested += 1;
        }
        if metrics.average_processing_time_ms == 0 {
            metrics.average_processing_time_ms = elapsed_ms;
        } else {
            metrics.average_processing_time_ms =
                (metrics.average_processing_time_ms + elapsed_ms) / 2;
        }
        let session_count = self.sessions.read().len() as u64;
        if session_count > metrics.peak_sessions {
            metrics.peak_sessions = session_count;
        }
    }

    fn handle_unknown_intent(
        &self,
        text: &str,
        _session_id: &str,
    ) -> Result<AutomationResponse, BridgeError> {
        let config = self.config.read();
        if !config.enable_clarification {
            return Ok(AutomationResponse::error(format!(
                "unrecognized request: {}",
                text
            )));
        }

        let suggestions = match text.to_lowercase().split_whitespace().next() {
            Some("open" | "launch" | "start") => {
                vec!["Try specifying the application name, e.g. 'open chrome'".into()]
            }
            Some("search" | "find") => {
                vec!["Try specifying what to search for, e.g. 'search for weather'".into()]
            }
            Some("click" | "tap" | "press") => {
                vec!["Try specifying what to click, e.g. 'click the submit button'".into()]
            }
            Some("type" | "enter") => {
                vec!["Try specifying what to type, e.g. 'type hello world'".into()]
            }
            _ => vec![
                "Try rephrasing your request".into(),
                "I can open apps, search, click, type, scroll, and more".into(),
            ],
        };

        Ok(AutomationResponse::success(
            AutomationDecision::Clarify {
                question: format!(
                    "I'm not sure what you mean by '{}'. Could you be more specific?",
                    text
                ),
                options: suggestions.clone(),
            },
            "clarification needed",
        )
        .with_suggestion("Try: 'open chrome'")
        .with_suggestion("Try: 'search for weather'"))
    }
}

unsafe impl Send for AIAutomationBridge {}
unsafe impl Sync for AIAutomationBridge {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution_manager::ExecutionManager;
    use crate::goal_registry::GoalRegistry;
    use crate::intention_parser::IntentParser;
    use nova_ai::AIEngine;

    fn make_test_kernel() -> Arc<nova_kernel::Kernel> {
        use nova_kernel::{
            consent::ConsentManager,
            egress::{EgressGate, EgressPolicy},
            event_bus::EventBus,
            module::ModuleRegistry,
            Kernel,
        };
        Arc::new(Kernel {
            event_bus: Arc::new(EventBus::new(16)),
            consent: Arc::new(ConsentManager::new()),
            egress_gate: Arc::new(EgressGate::new(
                Arc::new(ConsentManager::new()),
                EgressPolicy::OfflineOnly,
            )),
            registry: Arc::new(ModuleRegistry::new()),
            config_dir: std::env::temp_dir().join("nova_ai_bridge_test_config"),
            log_dir: std::env::temp_dir().join("nova_ai_bridge_test_logs"),
        })
    }

    fn make_ai_engine() -> Arc<AIEngine> {
        let kernel = make_test_kernel();
        Arc::new(AIEngine::new(kernel))
    }

    fn make_execution_manager() -> Arc<ExecutionManager> {
        let planner = Arc::new(Planner::new());
        let ws = Arc::new(parking_lot::RwLock::new(
            crate::world_state::WorldState::new(),
        ));
        let verifier = Arc::new(crate::outcome_verifier::OutcomeVerifier::new(
            ws.clone(),
            None,
        ));
        let orch = Arc::new(crate::recovery_orchestrator::RecoveryOrchestrator::new());
        let plan_executor = Arc::new(crate::plan_executor::PlanExecutor::new(
            crate::planner::Planner::new(),
            verifier,
            orch,
            ws,
        ));
        Arc::new(ExecutionManager::new(planner, plan_executor))
    }

    fn make_bridge() -> AIAutomationBridge {
        let ai = make_ai_engine();
        let parser = Arc::new(IntentParser::new());
        let registry = Arc::new(GoalRegistry::with_builtins());
        let planner = Arc::new(Planner::new());
        let exec_mgr = make_execution_manager();

        AIAutomationBridge::new(ai, parser, registry, planner, exec_mgr)
    }

    #[test]
    fn test_execute_simple_request() {
        let bridge = make_bridge();
        let request = AutomationRequest::new("open chrome");
        let response = bridge.execute(request).unwrap();
        assert!(response.success);
        match &response.decision {
            AutomationDecision::Execute {
                execution_id: _,
                goal,
            } => {
                assert!(goal.contains("open") || goal.contains("chrome"));
            }
            other => panic!("expected Execute, got {:?}", other),
        }
    }

    #[test]
    fn test_execute_empty_request() {
        let bridge = make_bridge();
        let request = AutomationRequest::new("");
        let result = bridge.execute(request);
        assert!(result.is_err());
        match result {
            Err(BridgeError::InvalidRequest(_)) => {}
            _ => panic!("expected InvalidRequest error"),
        }
    }

    #[test]
    fn test_execute_unknown_request() {
        let bridge = make_bridge();
        let request = AutomationRequest::new("xyzzy flurbo garblex");
        let response = bridge.execute(request).unwrap();
        match &response.decision {
            AutomationDecision::Clarify {
                question: _,
                options: _,
            } => {
                assert!(!response.success || response.message.contains("clarification"));
            }
            AutomationDecision::Error { message: _ } => {}
            other => panic!("expected Clarify or Error, got {:?}", other),
        }
    }

    #[test]
    fn test_execute_search_request() {
        let bridge = make_bridge();
        let request = AutomationRequest::new("search for rust programming");
        let response = bridge.execute(request).unwrap();
        assert!(
            response.success || matches!(response.decision, AutomationDecision::Execute { .. })
        );
    }

    #[test]
    fn test_session_management() {
        let bridge = make_bridge();
        let request = AutomationRequest::new("open chrome").with_session("sess-1");
        let response = bridge.execute(request).unwrap();
        assert!(response.success);

        let session = bridge.session("sess-1");
        assert!(session.is_some());
        let session = session.unwrap();
        assert_eq!(session.id, "sess-1");
        assert!(!session.history.is_empty());
    }

    #[test]
    fn test_session_context_reset() {
        let bridge = make_bridge();
        let request = AutomationRequest::new("open chrome").with_session("reset-1");
        let _ = bridge.execute(request).unwrap();

        let session = bridge.session("reset-1").unwrap();
        assert!(!session.history.is_empty());

        bridge.reset_context("reset-1").unwrap();
        let session = bridge.session("reset-1").unwrap();
        assert!(session.history.is_empty());
    }

    #[test]
    fn test_reset_nonexistent_session() {
        let bridge = make_bridge();
        let result = bridge.reset_context("nonexistent");
        assert!(result.is_err());
        match result {
            Err(BridgeError::SessionNotFound(_)) => {}
            _ => panic!("expected SessionNotFound error"),
        }
    }

    #[test]
    fn test_cancel_execution() {
        let bridge = make_bridge();
        let response = bridge.cancel("nonexistent-id");
        assert!(response.is_ok());
        match response.unwrap().decision {
            AutomationDecision::Cancelled { .. } => {}
            AutomationDecision::Error { .. } => {}
            _ => panic!("expected Cancelled or Error"),
        }
    }

    #[test]
    fn test_capabilities() {
        let bridge = make_bridge();
        let caps = bridge.capabilities();
        assert!(caps.contains(&AutomationCapability::NaturalLanguage));
        assert!(caps.contains(&AutomationCapability::MultiStep));
        assert!(caps.contains(&AutomationCapability::Cancellation));
        assert!(caps.contains(&AutomationCapability::Clarification));
        assert!(caps.contains(&AutomationCapability::ContextAware));
        assert!(caps.contains(&AutomationCapability::PriorityScheduling));
    }

    #[test]
    fn test_metrics_initial() {
        let bridge = make_bridge();
        let metrics = bridge.metrics();
        assert_eq!(metrics.total_requests, 0);
        assert_eq!(metrics.successful_executions, 0);
        assert_eq!(metrics.failed_executions, 0);
    }

    #[test]
    fn test_metrics_after_execution() {
        let bridge = make_bridge();
        let request = AutomationRequest::new("open chrome");
        let _ = bridge.execute(request).unwrap();
        let metrics = bridge.metrics();
        assert!(metrics.total_requests >= 1);
    }

    #[test]
    fn test_execute_with_priority() {
        let bridge = make_bridge();
        let request = AutomationRequest::new("open chrome").with_priority(ExecutionPriority::High);
        let response = bridge.execute(request).unwrap();
        assert!(response.success);
    }

    #[test]
    fn test_execute_with_context() {
        let bridge = make_bridge();
        let ctx = AutomationContext {
            active_goal: Some("test goal".into()),
            last_intent: Some("open".into()),
            execution_count: 5,
            previous_goals: vec!["open notepad".into()],
            user_preferences: HashMap::new(),
        };
        let request = AutomationRequest::new("open chrome").with_context(ctx);
        let response = bridge.execute(request).unwrap();
        assert!(response.success);
    }

    #[test]
    fn test_multiple_sessions() {
        let bridge = make_bridge();
        let r1 = AutomationRequest::new("open chrome").with_session("multi-1");
        let r2 = AutomationRequest::new("open notepad").with_session("multi-2");

        let _ = bridge.execute(r1).unwrap();
        let _ = bridge.execute(r2).unwrap();

        let s1 = bridge.session("multi-1").unwrap();
        let s2 = bridge.session("multi-2").unwrap();
        assert_eq!(s1.id, "multi-1");
        assert_eq!(s2.id, "multi-2");
    }

    #[test]
    fn test_session_timestamps() {
        let bridge = make_bridge();
        let request = AutomationRequest::new("open chrome").with_session("ts-1");
        let _ = bridge.execute(request).unwrap();

        let session = bridge.session("ts-1").unwrap();
        assert!(session.created_at <= session.updated_at);
        assert!(session.created_at > 0);
    }

    #[test]
    fn test_context_previous_goals() {
        let bridge = make_bridge();
        let r1 = AutomationRequest::new("open chrome").with_session("pg-1");
        let r2 = AutomationRequest::new("open notepad").with_session("pg-1");

        let _ = bridge.execute(r1).unwrap();
        let _ = bridge.execute(r2).unwrap();

        let session = bridge.session("pg-1").unwrap();
        assert!(!session.context.previous_goals.is_empty());
    }

    #[test]
    fn test_serialization_roundtrip() {
        let intent = AutomationIntent::Execute;
        let json = serde_json::to_string(&intent).unwrap();
        let deserialized: AutomationIntent = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, AutomationIntent::Execute);

        let cap = AutomationCapability::NaturalLanguage;
        let json = serde_json::to_string(&cap).unwrap();
        let deserialized: AutomationCapability = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, AutomationCapability::NaturalLanguage);
    }

    #[test]
    fn test_decision_execute_variants() {
        let execute = AutomationDecision::Execute {
            execution_id: "e1".into(),
            goal: "open chrome".into(),
        };
        match &execute {
            AutomationDecision::Execute { execution_id, goal } => {
                assert_eq!(execution_id, "e1");
                assert_eq!(goal, "open chrome");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_decision_clarify_variants() {
        let clarify = AutomationDecision::Clarify {
            question: "what?".into(),
            options: vec!["a".into(), "b".into()],
        };
        match &clarify {
            AutomationDecision::Clarify { question, options } => {
                assert_eq!(question, "what?");
                assert_eq!(options.len(), 2);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_decision_error_variants() {
        let error = AutomationDecision::Error {
            message: "something went wrong".into(),
        };
        match &error {
            AutomationDecision::Error { message } => {
                assert_eq!(message, "something went wrong");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_request_builder() {
        let request = AutomationRequest::new("open chrome")
            .with_session("s1")
            .with_priority(ExecutionPriority::Immediate);
        assert_eq!(request.text, "open chrome");
        assert_eq!(request.session_id, Some("s1".into()));
        assert_eq!(request.priority, Some(ExecutionPriority::Immediate));
    }

    #[test]
    fn test_response_builder() {
        let response = AutomationResponse::success(
            AutomationDecision::Execute {
                execution_id: "e1".into(),
                goal: "test".into(),
            },
            "done",
        )
        .with_execution_id("e1")
        .with_suggestion("try something else");

        assert!(response.success);
        assert_eq!(response.execution_id, Some("e1".into()));
        assert!(!response.suggestions.is_empty());
    }

    #[test]
    fn test_error_display() {
        let e = BridgeError::InvalidRequest("bad".into());
        assert_eq!(e.to_string(), "invalid request: bad");

        let e = BridgeError::SessionNotFound("s1".into());
        assert_eq!(e.to_string(), "session not found: s1");
    }

    #[test]
    fn test_concurrent_sessions() {
        let bridge = Arc::new(make_bridge());
        let mut handles = Vec::new();

        for i in 0..10 {
            let b = bridge.clone();
            handles.push(std::thread::spawn(move || {
                let request =
                    AutomationRequest::new("open chrome").with_session(format!("conc-sess-{}", i));
                let _ = b.execute(request);
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        let metrics = bridge.metrics();
        assert_eq!(metrics.total_requests, 10);
    }

    #[test]
    fn test_config_set_get() {
        let bridge = make_bridge();
        let config = AutomationBridgeConfig {
            default_session_ttl_ms: 1000,
            max_session_history: 10,
            enable_clarification: false,
            auto_execute_high_confidence: false,
            minimum_confidence_threshold: 0.9,
        };
        bridge.set_config(config.clone());
        let retrieved = bridge.config();
        assert_eq!(retrieved.default_session_ttl_ms, 1000);
        assert!(!retrieved.enable_clarification);
    }

    #[test]
    fn test_planner_invocation() {
        let bridge = make_bridge();
        let request = AutomationRequest::new("set brightness to 75");
        let response = bridge.execute(request).unwrap();
        assert!(response.success);
    }

    #[test]
    fn test_execution_manager_invocation() {
        let bridge = make_bridge();
        let request = AutomationRequest::new("lock device");
        let response = bridge.execute(request).unwrap();
        assert!(response.success);
    }

    #[test]
    fn test_unknown_intent_no_clarification() {
        let bridge = make_bridge();
        let mut config = bridge.config();
        config.enable_clarification = false;
        bridge.set_config(config);

        let request = AutomationRequest::new("xyzzy");
        let response = bridge.execute(request).unwrap();
        match &response.decision {
            AutomationDecision::Error { message } => {
                assert!(message.contains("unrecognized"));
            }
            _ => panic!("expected Error"),
        }
    }

    #[test]
    fn test_multi_step_request() {
        let bridge = make_bridge();
        let request = AutomationRequest::new("open chrome and search for weather");
        let response = bridge.execute(request).unwrap();
        // Multi-step should still produce some result
        assert!(
            response.success
                || matches!(response.decision, AutomationDecision::Clarify { .. })
                || matches!(response.decision, AutomationDecision::Execute { .. })
        );
    }

    #[test]
    fn test_session_entry_count() {
        let bridge = make_bridge();
        let sid = "entry-count";
        for _i in 0..3 {
            let request = AutomationRequest::new("open chrome").with_session(sid);
            let _ = bridge.execute(request).unwrap();
        }
        let session = bridge.session(sid).unwrap();
        assert_eq!(session.history.len(), 3);
    }

    #[test]
    fn test_context_reset_clears_execution_count() {
        let bridge = make_bridge();
        let sid = "reset-count";
        let r1 = AutomationRequest::new("open chrome").with_session(sid);
        let _ = bridge.execute(r1).unwrap();
        bridge.reset_context(sid).unwrap();
        let session = bridge.session(sid).unwrap();
        assert_eq!(session.context.execution_count, 0);
    }

    #[test]
    fn test_execute_with_default_session() {
        let bridge = make_bridge();
        let request = AutomationRequest::new("open chrome");
        let response = bridge.execute(request).unwrap();
        assert!(response.success);
        let session = bridge.session("default");
        assert!(session.is_some());
    }
}
