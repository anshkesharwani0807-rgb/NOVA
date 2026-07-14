use std::collections::HashMap;
use std::sync::Arc;

use nova_automation::*;
use nova_kernel::EventBus;

// ═══════════════════════════════════════════════════════════════════════════════
// Test 1: Workflow Creation & Registration
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_workflow_creation_and_registration() {
    let mut wf = Workflow::new(
        "wf-001".to_string(),
        "Test Workflow".to_string(),
        "A test workflow".to_string(),
    );
    wf.enabled = true;
    wf.triggers.push(TriggerConfig {
        trigger: TriggerType::Manual,
        conditions: None,
    });
    wf.steps.push(WorkflowStep {
        id: "step-1".into(),
        name: "Speak".into(),
        action: ActionType::Speak {
            text: "Hello".into(),
        },
        condition: None,
        retry_count: 0,
        timeout_ms: 5000,
        continue_on_failure: false,
    });
    wf.parallel = false;
    wf.max_retries = 1;
    wf.timeout_ms = 10000;
    wf.tags = vec!["test".into(), "integration".into()];

    let registry = WorkflowRegistry::new();
    assert!(registry.register(wf.clone()).is_ok());

    let retrieved = registry.get("wf-001").expect("workflow should exist");
    assert_eq!(retrieved.name, "Test Workflow");
    assert_eq!(retrieved.id, "wf-001");
    assert_eq!(retrieved.steps.len(), 1);
    assert_eq!(retrieved.triggers.len(), 1);
    assert!(matches!(retrieved.triggers[0].trigger, TriggerType::Manual));

    let dup = registry.register(wf);
    assert!(dup.is_err());
    assert!(matches!(
        dup,
        Err(AutomationError::WorkflowAlreadyExists(_))
    ));

    let list = registry.list();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name, "Test Workflow");
    assert_eq!(list[0].step_count, 1);
    assert_eq!(list[0].trigger_count, 1);
}

// ═══════════════════════════════════════════════════════════════════════════════
// Test 2: Workflow CRUD
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_workflow_crud() {
    let registry = WorkflowRegistry::new();

    let mut wf = Workflow::new("crud-1".into(), "CRUD Test".into(), "crud".into());
    wf.triggers.push(TriggerConfig {
        trigger: TriggerType::Manual,
        conditions: None,
    });
    registry.register(wf).unwrap();

    // Disable
    registry.disable("crud-1").unwrap();
    let wf = registry.get("crud-1").unwrap();
    assert!(!wf.enabled);
    let list = registry.list();
    assert_eq!(list[0].state, WorkflowState::Disabled);

    // Enable
    registry.enable("crud-1").unwrap();
    let wf = registry.get("crud-1").unwrap();
    assert!(wf.enabled);

    // Update
    let mut updated = Workflow::new("crud-1".into(), "CRUD Updated".into(), "updated".into());
    updated.steps.push(WorkflowStep {
        id: "s1".into(),
        name: "Notify".into(),
        action: ActionType::Notify {
            title: "hi".into(),
            body: "there".into(),
            priority: NotifyPriority::Normal,
        },
        condition: None,
        retry_count: 0,
        timeout_ms: 5000,
        continue_on_failure: false,
    });
    registry.update(updated).unwrap();
    let wf = registry.get("crud-1").unwrap();
    assert_eq!(wf.name, "CRUD Updated");
    assert_eq!(wf.steps.len(), 1);

    // Delete
    registry.delete("crud-1").unwrap();
    assert!(registry.get("crud-1").is_none());
    assert_eq!(registry.count(), 0);

    // Delete non-existent
    let err = registry.delete("nonexistent");
    assert!(err.is_err());
    assert!(matches!(err, Err(AutomationError::WorkflowNotFound(_))));
}

// ═══════════════════════════════════════════════════════════════════════════════
// Test 3: Trigger Types
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_trigger_type_variants() {
    let triggers = vec![
        TriggerType::Time {
            hour: 8,
            minute: 0,
            days_of_week: Some(vec![1, 2, 3, 4, 5]),
        },
        TriggerType::Date {
            year: Some(2026),
            month: Some(12),
            day: Some(25),
        },
        TriggerType::Battery {
            level: 20,
            above: false,
        },
        TriggerType::Charging {
            state: ChargingState::Charging,
        },
        TriggerType::WiFi {
            ssid: Some("Home".into()),
            connected: true,
        },
        TriggerType::Bluetooth {
            device_name: Some("Headphones".into()),
            connected: true,
        },
        TriggerType::DeviceState {
            state: DeviceState::ScreenOn,
        },
        TriggerType::Memory {
            category: Some("notes".into()),
            keyword: Some("todo".into()),
            event: "created".into(),
        },
        TriggerType::Voice {
            phrase: "hey nova".into(),
        },
        TriggerType::Vision {
            event: "face_detected".into(),
        },
        TriggerType::Manual,
        TriggerType::EventBus {
            event_name: "custom.event".into(),
            filter: Some(HashMap::from([("key".into(), "val".into())])),
        },
        TriggerType::Plugin {
            plugin_id: "lights".into(),
            event: "toggle".into(),
        },
    ];

    // Each variant should be serializable/deserializable
    for trigger in &triggers {
        let json = serde_json::to_string(trigger).unwrap();
        let _back: TriggerType = serde_json::from_str(&json).unwrap();
    }

    assert_eq!(triggers.len(), 13);
}

#[test]
fn test_trigger_config_creation() {
    let configs = vec![
        TriggerConfig {
            trigger: TriggerType::Manual,
            conditions: None,
        },
        TriggerConfig {
            trigger: TriggerType::Voice {
                phrase: "hello".into(),
            },
            conditions: Some(vec![Condition::True]),
        },
        TriggerConfig {
            trigger: TriggerType::EventBus {
                event_name: "test".into(),
                filter: None,
            },
            conditions: Some(vec![Condition::Comparison {
                field: "level".into(),
                operator: ComparisonOp::Gt,
                value: "3".into(),
            }]),
        },
    ];

    assert_eq!(configs.len(), 3);

    let json = serde_json::to_string(&configs).unwrap();
    let _back: Vec<TriggerConfig> = serde_json::from_str(&json).unwrap();
}

#[test]
fn test_manual_trigger_evaluation() {
    let evaluator = ManualTriggerEvaluator;
    let ctx = HashMap::new();

    let result = evaluator.evaluate(&TriggerType::Manual, &ctx);
    assert!(result.triggered);
    assert_eq!(result.reason, "manual trigger");

    let result = evaluator.evaluate(
        &TriggerType::Time {
            hour: 12,
            minute: 0,
            days_of_week: None,
        },
        &ctx,
    );
    assert!(!result.triggered);

    assert_eq!(evaluator.kind(), "manual");
}

#[test]
fn test_time_trigger_evaluator_kind() {
    let evaluator = TimeTriggerEvaluator;
    assert_eq!(evaluator.kind(), "time");
}

#[test]
fn test_event_bus_trigger_evaluator() {
    let evaluator = EventBusTriggerEvaluator;
    assert_eq!(evaluator.kind(), "event_bus");

    let mut ctx = HashMap::new();
    ctx.insert("event_name".into(), "my.event".into());

    let result = evaluator.evaluate(
        &TriggerType::EventBus {
            event_name: "my.event".into(),
            filter: None,
        },
        &ctx,
    );
    assert!(result.triggered);

    let result = evaluator.evaluate(
        &TriggerType::EventBus {
            event_name: "other.event".into(),
            filter: None,
        },
        &ctx,
    );
    assert!(!result.triggered);

    // With filter match
    ctx.insert("key1".into(), "val1".into());
    let result = evaluator.evaluate(
        &TriggerType::EventBus {
            event_name: "my.event".into(),
            filter: Some(HashMap::from([("key1".into(), "val1".into())])),
        },
        &ctx,
    );
    assert!(result.triggered);

    // With filter mismatch
    let result = evaluator.evaluate(
        &TriggerType::EventBus {
            event_name: "my.event".into(),
            filter: Some(HashMap::from([("key1".into(), "wrong".into())])),
        },
        &ctx,
    );
    assert!(!result.triggered);
}

// ═══════════════════════════════════════════════════════════════════════════════
// Test 4: Trigger Evaluation with Scheduler
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_scheduler_manual_triggers() {
    let config = AutomationConfig::default();
    let mut scheduler = Scheduler::new(config);
    scheduler.register_evaluator(Box::new(ManualTriggerEvaluator));

    let mut wf_a = Workflow::new("a".into(), "A".into(), "workflow A".into());
    wf_a.triggers.push(TriggerConfig {
        trigger: TriggerType::Manual,
        conditions: None,
    });
    wf_a.enabled = true;

    let mut wf_b = Workflow::new("b".into(), "B".into(), "workflow B".into());
    wf_b.triggers.push(TriggerConfig {
        trigger: TriggerType::Time {
            hour: 99,
            minute: 99,
            days_of_week: None,
        },
        conditions: None,
    });
    wf_b.enabled = true;

    let wf_disabled = Workflow::new("c".into(), "C".into(), "disabled".into());
    // wf_disabled is enabled by default - disable it
    let wf_disabled = {
        let mut w = wf_disabled;
        w.enabled = false;
        w.triggers.push(TriggerConfig {
            trigger: TriggerType::Manual,
            conditions: None,
        });
        w
    };

    let workflows: Vec<Arc<Workflow>> = vec![Arc::new(wf_a), Arc::new(wf_b), Arc::new(wf_disabled)];

    let ctx = HashMap::new();
    let results = scheduler.check_triggers(&workflows, &ctx);

    // Only wf_a should trigger (manual trigger matched, workflow enabled)
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].0.id, "a");
    assert!(results[0].1.triggered);

    // No time-based triggers -> no next scheduled
    let next = scheduler.get_next_scheduled(&workflows);
    // wf_b has hour=99 which makes no sense, but get_next_scheduled will try
    // and return Some if it can construct a future timestamp
    // Actually hour=99 will fail with_hour and return None, so next should be None
    assert!(next.is_none());
}

#[test]
fn test_scheduler_empty_workflows() {
    let config = AutomationConfig::default();
    let scheduler = Scheduler::new(config);
    let ctx = HashMap::new();

    let results = scheduler.check_triggers(&[], &ctx);
    assert!(results.is_empty());

    let next = scheduler.get_next_scheduled(&[]);
    assert!(next.is_none());
}

#[test]
fn test_scheduler_disabled_workflows_skipped() {
    let config = AutomationConfig::default();
    let scheduler = Scheduler::new(config);

    let mut wf = Workflow::new("d".into(), "D".into(), "disabled".into());
    wf.enabled = false;
    wf.triggers.push(TriggerConfig {
        trigger: TriggerType::Manual,
        conditions: None,
    });

    let workflows = vec![Arc::new(wf)];
    let ctx = HashMap::new();
    let results = scheduler.check_triggers(&workflows, &ctx);
    assert!(results.is_empty());
}

// ═══════════════════════════════════════════════════════════════════════════════
// Test 5: Action Execution
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_default_action_executor_all_types() {
    let executor = DefaultActionExecutor;

    let actions: Vec<(ActionType, &str)> = vec![
        (
            ActionType::Speak {
                text: "hello".into(),
            },
            "speak: hello",
        ),
        (
            ActionType::Notify {
                title: "Alert".into(),
                body: "Body".into(),
                priority: NotifyPriority::High,
            },
            "notify: Alert - Body",
        ),
        (
            ActionType::OpenApp {
                app_id: "com.test.app".into(),
                data: None,
            },
            "open app: com.test.app",
        ),
        (
            ActionType::LaunchActivity {
                package: "com.test".into(),
                activity: ".Main".into(),
                data: None,
            },
            "launch com.test/.Main",
        ),
        (
            ActionType::Clipboard {
                action: ClipboardAction::Copy,
                text: Some("data".into()),
            },
            "clipboard: Copy",
        ),
        (
            ActionType::CreateMemory {
                title: "Note".into(),
                content: "content".into(),
                category: "personal".into(),
                tags: vec![],
                importance: 5,
            },
            "memory created: 'Note' in personal",
        ),
        (
            ActionType::SearchMemory {
                query: "rust".into(),
                max_results: 10,
            },
            "search 'rust' (max 10)",
        ),
        (
            ActionType::RunAI {
                prompt: "Hello world".into(),
                session_id: None,
            },
            "ai inference: Hello world",
        ),
        (
            ActionType::CaptureVoice {
                duration_secs: Some(30),
            },
            "voice capture: 30s",
        ),
        (
            ActionType::AnalyzeImage {
                image_path: "/tmp/photo.jpg".into(),
                analysis_type: "objects".into(),
            },
            "analyze objects: /tmp/photo.jpg",
        ),
        (
            ActionType::DeviceControl {
                control: DeviceControl::SetBrightness(75),
            },
            "device control: SetBrightness(75)",
        ),
        (
            ActionType::PluginInvocation {
                plugin_id: "home".into(),
                method: "turn_on".into(),
                parameters: HashMap::from([("room".into(), "living".into())]),
            },
            "plugin home.turn_on",
        ),
        (ActionType::Wait { duration_ms: 1 }, "waited 1ms"),
        (
            ActionType::SubWorkflow {
                workflow_id: "sub-1".into(),
            },
            "sub-workflow: sub-1",
        ),
    ];

    for (action, expected_msg) in &actions {
        let result = executor.execute(action);
        assert!(result.success, "action {:?} should succeed", action);
        assert_eq!(result.message, *expected_msg);
    }
}

#[test]
fn test_action_result_constructors() {
    let s = ActionResult::success("done");
    assert!(s.success);
    assert_eq!(s.message, "done");
    assert!(s.data.is_none());

    let sd = ActionResult::success_with_data("done", "payload");
    assert!(sd.success);
    assert_eq!(sd.data, Some("payload".into()));

    let f = ActionResult::failure("fail");
    assert!(!f.success);
    assert_eq!(f.message, "fail");
}

// ═══════════════════════════════════════════════════════════════════════════════
// Test 6: Condition Evaluation
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_condition_evaluation_all_variants() {
    let evaluator = DefaultConditionEvaluator;
    let mut ctx = HashMap::new();
    ctx.insert("name".into(), "Alice".into());
    ctx.insert("age".into(), "30".into());
    ctx.insert("email".into(), "alice@example.com".into());
    ctx.insert("date".into(), "2026-07-14".into());

    // Comparison - Eq match
    let result = evaluator.evaluate(
        &Condition::Comparison {
            field: "name".into(),
            operator: ComparisonOp::Eq,
            value: "Alice".into(),
        },
        &ctx,
    );
    assert!(result.matched);

    // Comparison - Neq match
    let result = evaluator.evaluate(
        &Condition::Comparison {
            field: "name".into(),
            operator: ComparisonOp::Neq,
            value: "Bob".into(),
        },
        &ctx,
    );
    assert!(result.matched);

    // Comparison - Gt
    let result = evaluator.evaluate(
        &Condition::Comparison {
            field: "name".into(),
            operator: ComparisonOp::Gt,
            value: "Adam".into(),
        },
        &ctx,
    );
    assert!(result.matched);

    // Regex match
    let result = evaluator.evaluate(
        &Condition::Regex {
            field: "email".into(),
            pattern: r"^alice@".into(),
        },
        &ctx,
    );
    assert!(result.matched);

    // Regex no match
    let result = evaluator.evaluate(
        &Condition::Regex {
            field: "email".into(),
            pattern: r"^bob@".into(),
        },
        &ctx,
    );
    assert!(!result.matched);

    // Contains
    let result = evaluator.evaluate(
        &Condition::Contains {
            field: "name".into(),
            value: "Ali".into(),
        },
        &ctx,
    );
    assert!(result.matched);

    // Not contains
    let result = evaluator.evaluate(
        &Condition::Contains {
            field: "name".into(),
            value: "Bob".into(),
        },
        &ctx,
    );
    assert!(!result.matched);

    // Numeric - Eq
    let result = evaluator.evaluate(
        &Condition::Numeric {
            field: "age".into(),
            operator: NumericOp::Eq,
            value: 30.0,
        },
        &ctx,
    );
    assert!(result.matched);

    // Numeric - Gt
    let result = evaluator.evaluate(
        &Condition::Numeric {
            field: "age".into(),
            operator: NumericOp::Gt,
            value: 20.0,
        },
        &ctx,
    );
    assert!(result.matched);

    // Numeric - Between
    let result = evaluator.evaluate(
        &Condition::Numeric {
            field: "age".into(),
            operator: NumericOp::Between {
                min: 25.0,
                max: 35.0,
            },
            value: 0.0,
        },
        &ctx,
    );
    assert!(result.matched);

    // DateCompare
    let result = evaluator.evaluate(
        &Condition::DateCompare {
            field: "date".into(),
            operator: ComparisonOp::Eq,
            value: "2026-07-14".into(),
        },
        &ctx,
    );
    assert!(result.matched);

    // PermissionCheck - granted
    let mut perm_ctx = HashMap::new();
    perm_ctx.insert("permissions".into(), "read,write,execute".into());
    let result = evaluator.evaluate(
        &Condition::PermissionCheck {
            permission: "write".into(),
        },
        &perm_ctx,
    );
    assert!(result.matched);

    // PermissionCheck - denied
    let result = evaluator.evaluate(
        &Condition::PermissionCheck {
            permission: "admin".into(),
        },
        &perm_ctx,
    );
    assert!(!result.matched);

    // ContextCheck - exists true
    let result = evaluator.evaluate(
        &Condition::ContextCheck {
            key: "name".into(),
            exists: true,
        },
        &ctx,
    );
    assert!(result.matched);

    // ContextCheck - exists false
    let result = evaluator.evaluate(
        &Condition::ContextCheck {
            key: "missing_key".into(),
            exists: false,
        },
        &ctx,
    );
    assert!(result.matched);

    // True
    let result = evaluator.evaluate(&Condition::True, &ctx);
    assert!(result.matched);

    // False
    let result = evaluator.evaluate(&Condition::False, &ctx);
    assert!(!result.matched);

    // And
    let result = evaluator.evaluate(
        &Condition::And(vec![
            Condition::True,
            Condition::Comparison {
                field: "name".into(),
                operator: ComparisonOp::Eq,
                value: "Alice".into(),
            },
        ]),
        &ctx,
    );
    assert!(result.matched);

    // And fails
    let result = evaluator.evaluate(
        &Condition::And(vec![Condition::True, Condition::False]),
        &ctx,
    );
    assert!(!result.matched);

    // Or succeeds
    let result = evaluator.evaluate(
        &Condition::Or(vec![Condition::False, Condition::True]),
        &ctx,
    );
    assert!(result.matched);

    // Or fails
    let result = evaluator.evaluate(
        &Condition::Or(vec![Condition::False, Condition::False]),
        &ctx,
    );
    assert!(!result.matched);

    // Not
    let result = evaluator.evaluate(&Condition::Not(Box::new(Condition::False)), &ctx);
    assert!(result.matched);

    let result = evaluator.evaluate(&Condition::Not(Box::new(Condition::True)), &ctx);
    assert!(!result.matched);
}

// ═══════════════════════════════════════════════════════════════════════════════
// Test 7: Workflow Execution (End-to-End)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_workflow_execution_end_to_end() {
    let config = AutomationConfig::default();
    let history = Arc::new(InMemoryHistory::new());
    let engine = ExecutionEngine::new(config, history.clone());

    let mut wf = Workflow::new("e2e-1".into(), "E2E".into(), "end to end test".into());
    wf.steps.push(WorkflowStep {
        id: "step1".into(),
        name: "Speak".into(),
        action: ActionType::Speak {
            text: "hello world".into(),
        },
        condition: None,
        retry_count: 0,
        timeout_ms: 5000,
        continue_on_failure: false,
    });

    let ctx = HashMap::new();
    let execution_id = engine.execute_workflow(Arc::new(wf), ctx, &|_| {});

    assert!(!execution_id.is_empty());

    let records = history.recent(10);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].execution_id, execution_id);
    assert_eq!(records[0].workflow_id, "e2e-1");
    assert_eq!(records[0].status, ExecutionStatus::Completed);
    assert_eq!(records[0].steps_total, 1);
    assert_eq!(records[0].steps_succeeded, 1);
    assert_eq!(records[0].steps_failed, 0);
    assert!(records[0].duration_ms >= 0);
    assert!(records[0].error.is_none());
}

// ═══════════════════════════════════════════════════════════════════════════════
// Test 8: Parallel Execution
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_parallel_execution() {
    let config = AutomationConfig::default();
    let history = Arc::new(InMemoryHistory::new());
    let engine = ExecutionEngine::new(config, history);

    let mut wf = Workflow::new(
        "parallel-1".into(),
        "Parallel".into(),
        "parallel test".into(),
    );
    wf.parallel = true;
    wf.steps.push(WorkflowStep {
        id: "s1".into(),
        name: "Speak1".into(),
        action: ActionType::Speak {
            text: "first".into(),
        },
        condition: None,
        retry_count: 0,
        timeout_ms: 5000,
        continue_on_failure: false,
    });
    wf.steps.push(WorkflowStep {
        id: "s2".into(),
        name: "Speak2".into(),
        action: ActionType::Speak {
            text: "second".into(),
        },
        condition: None,
        retry_count: 0,
        timeout_ms: 5000,
        continue_on_failure: false,
    });
    wf.steps.push(WorkflowStep {
        id: "s3".into(),
        name: "Speak3".into(),
        action: ActionType::Speak {
            text: "third".into(),
        },
        condition: None,
        retry_count: 0,
        timeout_ms: 5000,
        continue_on_failure: false,
    });

    let events: Arc<std::sync::Mutex<Vec<AutomationEventPayload>>> =
        Arc::new(std::sync::Mutex::new(Vec::new()));
    let events_clone = events.clone();
    let publish = move |payload: AutomationEventPayload| {
        events_clone.lock().unwrap().push(payload);
    };

    let ctx = HashMap::new();
    let execution_id = engine.execute_workflow(Arc::new(wf), ctx, &publish);

    assert!(!execution_id.is_empty());

    let captured = events.lock().unwrap();
    // The parent publish closure receives WorkflowStarted and WorkflowCompleted
    // (child parallel steps use a noop publish closure internally)
    let started_events: Vec<_> = captured
        .iter()
        .filter(|e| matches!(e, AutomationEventPayload::WorkflowStarted { .. }))
        .collect();
    assert_eq!(started_events.len(), 1);

    let completed_events: Vec<_> = captured
        .iter()
        .filter(|e| matches!(e, AutomationEventPayload::WorkflowCompleted { .. }))
        .collect();
    assert_eq!(completed_events.len(), 1);
}

// ═══════════════════════════════════════════════════════════════════════════════
// Test 9: Cancellation
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_cancel_nonexistent_execution() {
    let config = AutomationConfig::default();
    let history = Arc::new(InMemoryHistory::new());
    let engine = ExecutionEngine::new(config, history);

    let err = engine.cancel_execution("nonexistent");
    assert!(err.is_err());
    assert!(matches!(err, Err(AutomationError::WorkflowNotFound(_))));
}

#[test]
fn test_cancel_mid_execution() {
    let config = AutomationConfig::default();
    let history = Arc::new(InMemoryHistory::new());
    let engine = Arc::new(ExecutionEngine::new(config, history));
    let engine_clone = engine.clone();

    let mut wf = Workflow::new("cancel-wf".into(), "Cancel".into(), "cancel test".into());
    wf.steps.push(WorkflowStep {
        id: "wait".into(),
        name: "Wait".into(),
        action: ActionType::Wait { duration_ms: 2000 },
        condition: None,
        retry_count: 0,
        timeout_ms: 5000,
        continue_on_failure: true,
    });
    wf.steps.push(WorkflowStep {
        id: "after".into(),
        name: "AfterCancel".into(),
        action: ActionType::Speak {
            text: "after cancel".into(),
        },
        condition: None,
        retry_count: 0,
        timeout_ms: 5000,
        continue_on_failure: false,
    });

    let captured_id: Arc<std::sync::Mutex<Option<String>>> = Arc::new(std::sync::Mutex::new(None));
    let captured_id_clone = captured_id.clone();

    let publish = move |payload: AutomationEventPayload| {
        if let AutomationEventPayload::WorkflowStarted { execution_id, .. } = payload {
            *captured_id_clone.lock().unwrap() = Some(execution_id);
        }
    };

    let ctx = HashMap::new();
    let handle = std::thread::spawn(move || {
        engine_clone.execute_workflow(Arc::new(wf), ctx, &publish);
    });

    std::thread::sleep(std::time::Duration::from_millis(200));

    let eid = captured_id.lock().unwrap().clone();
    assert!(eid.is_some(), "execution should have started");

    let result = engine.cancel_execution(&eid.unwrap());
    assert!(result.is_ok(), "cancellation should succeed: {:?}", result);

    handle.join().unwrap();
    assert_eq!(engine.active_count(), 0);
}

// ═══════════════════════════════════════════════════════════════════════════════
// Test 10: History Recording
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_history_recording() {
    let history = InMemoryHistory::new();

    let record = |wf_id: &str, exec_id: &str, status: ExecutionStatus| {
        history.store(ExecutionRecord {
            execution_id: exec_id.to_string(),
            workflow_id: wf_id.to_string(),
            workflow_name: format!("wf-{}", wf_id),
            status,
            started_at: 1000,
            duration_ms: 500,
            steps_succeeded: 1,
            steps_failed: 0,
            steps_total: 1,
            error: None,
        });
    };

    record("wf-a", "exec-1", ExecutionStatus::Completed);
    record("wf-a", "exec-2", ExecutionStatus::Completed);
    record("wf-b", "exec-3", ExecutionStatus::Failed);
    record("wf-a", "exec-4", ExecutionStatus::Partial);

    // Test count
    assert_eq!(history.count(), 4);

    // Test get_recent
    let recent = history.recent(2);
    assert_eq!(recent.len(), 2);
    assert_eq!(recent[0].execution_id, "exec-4");
    assert_eq!(recent[1].execution_id, "exec-3");

    // Test by_workflow
    let wf_a_records = history.by_workflow("wf-a", 10);
    assert_eq!(wf_a_records.len(), 3);
    for r in &wf_a_records {
        assert_eq!(r.workflow_id, "wf-a");
    }

    // Test by_workflow with limit
    let wf_a_limited = history.by_workflow("wf-a", 2);
    assert_eq!(wf_a_limited.len(), 2);

    // Test by_status
    let completed = history.by_status(&ExecutionStatus::Completed, 10);
    assert_eq!(completed.len(), 2);

    let failed = history.by_status(&ExecutionStatus::Failed, 10);
    assert_eq!(failed.len(), 1);

    // Test clear
    history.clear();
    assert_eq!(history.count(), 0);
}

#[test]
fn test_history_max_entries() {
    let history = InMemoryHistory::with_max(3);

    for i in 0..5 {
        history.store(ExecutionRecord {
            execution_id: format!("exec-{}", i),
            workflow_id: "wf".into(),
            workflow_name: "test".into(),
            status: ExecutionStatus::Completed,
            started_at: i as i64,
            duration_ms: 0,
            steps_succeeded: 1,
            steps_failed: 0,
            steps_total: 1,
            error: None,
        });
    }

    assert_eq!(history.count(), 3);
    let recent = history.recent(5);
    assert_eq!(recent.len(), 3);
    assert_eq!(recent[0].execution_id, "exec-4");
    assert_eq!(recent[2].execution_id, "exec-2");
}

// ═══════════════════════════════════════════════════════════════════════════════
// Test 11: AutomationEngine Integration
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_automation_engine_create_workflow() {
    let engine = AutomationEngine::new();
    let wf = engine.create_workflow("My Workflow", "created via engine");
    assert_eq!(wf.name, "My Workflow");
    assert_eq!(wf.description, "created via engine");
    assert!(!wf.id.is_empty());
}

#[test]
fn test_automation_engine_register_and_trigger() {
    let bus = Arc::new(EventBus::new(64));
    let engine = AutomationEngine::new();
    engine.set_event_bus(bus);

    let mut wf = engine.create_workflow("Trigger Test", "test manual trigger");
    wf.triggers.push(TriggerConfig {
        trigger: TriggerType::Manual,
        conditions: None,
    });
    wf.steps.push(WorkflowStep {
        id: "step1".into(),
        name: "Speak".into(),
        action: ActionType::Speak {
            text: "triggered".into(),
        },
        condition: None,
        retry_count: 0,
        timeout_ms: 5000,
        continue_on_failure: false,
    });

    let wf_id = wf.id.clone();
    engine.register_workflow(wf).unwrap();

    let execution_id = engine.trigger_manual(&wf_id).unwrap();
    assert!(!execution_id.is_empty());

    let records = engine.history().recent(10);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].workflow_id, wf_id);
    assert_eq!(records[0].status, ExecutionStatus::Completed);
}

#[test]
fn test_automation_engine_cancel_execution() {
    let engine = AutomationEngine::new();

    let err = engine.cancel_execution("nonexistent-exec");
    assert!(err.is_err());
    assert!(matches!(err, Err(AutomationError::WorkflowNotFound(_))));
}

// ═══════════════════════════════════════════════════════════════════════════════
// Test 12: Event Bus Integration
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_event_bus_workflow_created_event() {
    let bus = Arc::new(EventBus::new(64));
    let mut rx = bus.subscribe();

    let engine = AutomationEngine::new();
    engine.set_event_bus(bus);

    let mut wf = engine.create_workflow("Event WF", "event bus test");
    wf.triggers.push(TriggerConfig {
        trigger: TriggerType::Manual,
        conditions: None,
    });
    let wf_id = wf.id.clone();
    let wf_name = wf.name.clone();
    engine.register_workflow(wf).unwrap();

    let received = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .expect("should receive event within timeout")
        .expect("channel should not be closed");

    let payload = received
        .payload
        .downcast_ref::<AutomationEventPayload>()
        .unwrap();
    match payload {
        AutomationEventPayload::WorkflowCreated { workflow_id, name } => {
            assert_eq!(workflow_id, &wf_id);
            assert_eq!(name, &wf_name);
        }
        other => panic!("expected WorkflowCreated, got {:?}", other),
    }
}

#[tokio::test]
async fn test_event_bus_trigger_activated_event() {
    let bus = Arc::new(EventBus::new(64));
    let mut rx = bus.subscribe();

    let engine = AutomationEngine::new();
    engine.set_event_bus(bus);

    let mut wf = engine.create_workflow("Trigger Event", "trigger event test");
    wf.triggers.push(TriggerConfig {
        trigger: TriggerType::Manual,
        conditions: None,
    });
    wf.steps.push(WorkflowStep {
        id: "s1".into(),
        name: "Speak".into(),
        action: ActionType::Speak { text: "hi".into() },
        condition: None,
        retry_count: 0,
        timeout_ms: 5000,
        continue_on_failure: false,
    });

    let wf_id = wf.id.clone();
    engine.register_workflow(wf).unwrap();

    // Consume the WorkflowCreated event
    let _created = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .expect("should get WorkflowCreated");

    // Now trigger
    engine.trigger_manual(&wf_id).unwrap();

    // We should get TriggerActivated, followed by WorkflowStarted, ActionExecuted, WorkflowCompleted
    let received = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .expect("should receive TriggerActivated within timeout")
        .expect("channel should not be closed");

    let payload = received
        .payload
        .downcast_ref::<AutomationEventPayload>()
        .unwrap();
    match payload {
        AutomationEventPayload::TriggerActivated {
            workflow_id,
            trigger_type,
            reason,
        } => {
            assert_eq!(workflow_id, &wf_id);
            assert_eq!(trigger_type, "manual");
            assert_eq!(reason, "manual trigger");
        }
        other => panic!("expected TriggerActivated, got {:?}", other),
    }
}

#[tokio::test]
async fn test_event_bus_multiple_events_on_execution() {
    let bus = Arc::new(EventBus::new(64));
    let mut rx = bus.subscribe();
    let engine = AutomationEngine::new();
    engine.set_event_bus(bus);

    let mut wf = engine.create_workflow("Multi Event", "multiple events");
    wf.triggers.push(TriggerConfig {
        trigger: TriggerType::Manual,
        conditions: None,
    });
    wf.steps.push(WorkflowStep {
        id: "s1".into(),
        name: "Speak".into(),
        action: ActionType::Speak {
            text: "multi".into(),
        },
        condition: None,
        retry_count: 0,
        timeout_ms: 5000,
        continue_on_failure: false,
    });

    let wf_id = wf.id.clone();
    engine.register_workflow(wf).unwrap();
    let _created = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .expect("WorkflowCreated");

    engine.trigger_manual(&wf_id).unwrap();

    let mut seen_trigger = false;
    let mut seen_started = false;
    let mut seen_action = false;
    let mut seen_completed = false;

    for _ in 0..4 {
        let event = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
            .await
            .expect("timeout waiting for event")
            .expect("channel closed");
        let payload = event
            .payload
            .downcast_ref::<AutomationEventPayload>()
            .unwrap();
        match payload {
            AutomationEventPayload::TriggerActivated { .. } => seen_trigger = true,
            AutomationEventPayload::WorkflowStarted { .. } => seen_started = true,
            AutomationEventPayload::ActionExecuted { .. } => seen_action = true,
            AutomationEventPayload::WorkflowCompleted { .. } => seen_completed = true,
            _ => {}
        }
    }

    assert!(seen_trigger, "should have seen TriggerActivated");
    assert!(seen_started, "should have seen WorkflowStarted");
    assert!(seen_action, "should have seen ActionExecuted");
    assert!(seen_completed, "should have seen WorkflowCompleted");
}

// ═══════════════════════════════════════════════════════════════════════════════
// Test 13: Permission / Auth Tests
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_workflow_disabled_error() {
    let engine = AutomationEngine::new();

    let mut wf = engine.create_workflow("Disabled WF", "should fail");
    wf.enabled = false;
    wf.triggers.push(TriggerConfig {
        trigger: TriggerType::Manual,
        conditions: None,
    });

    let wf_id = wf.id.clone();
    engine.register_workflow(wf).unwrap();

    let err = engine.trigger_manual(&wf_id);
    assert!(err.is_err());
    assert!(matches!(err, Err(AutomationError::WorkflowDisabled(_))));
}

#[test]
fn test_trigger_nonexistent_workflow() {
    let engine = AutomationEngine::new();

    let err = engine.trigger_manual("i-dont-exist");
    assert!(err.is_err());
    assert!(matches!(err, Err(AutomationError::WorkflowNotFound(_))));
}

// ═══════════════════════════════════════════════════════════════════════════════
// Test 14: Error Handling
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_invalid_workflow_registration() {
    let registry = WorkflowRegistry::new();

    // Empty ID
    let wf = Workflow::new(String::new(), "name".into(), "desc".into());
    let err = registry.register(wf).unwrap_err();
    assert!(matches!(err, AutomationError::InvalidWorkflow(_)));

    // Empty name
    let wf = Workflow::new("id-ok".into(), String::new(), "desc".into());
    let err = registry.register(wf).unwrap_err();
    assert!(matches!(err, AutomationError::InvalidWorkflow(_)));
}

#[test]
fn test_workflow_serialization_roundtrip() {
    let mut wf = Workflow::new("ser-1".into(), "Serialization".into(), "test".into());
    wf.steps.push(WorkflowStep {
        id: "s1".into(),
        name: "Notify".into(),
        action: ActionType::Notify {
            title: "Test".into(),
            body: "Body".into(),
            priority: NotifyPriority::Critical,
        },
        condition: Some(Condition::True),
        retry_count: 2,
        timeout_ms: 10000,
        continue_on_failure: true,
    });
    wf.tags = vec!["a".into(), "b".into()];

    let json = serde_json::to_string(&wf).unwrap();
    let deserialized: Workflow = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.id, wf.id);
    assert_eq!(deserialized.name, wf.name);
    assert_eq!(deserialized.description, wf.description);
    assert_eq!(deserialized.steps.len(), wf.steps.len());
    assert_eq!(deserialized.steps[0].id, wf.steps[0].id);
    assert_eq!(deserialized.tags, wf.tags);
}

#[test]
fn test_action_type_serialization() {
    let actions = vec![
        ActionType::Speak {
            text: "hello".into(),
        },
        ActionType::Notify {
            title: "T".into(),
            body: "B".into(),
            priority: NotifyPriority::Low,
        },
        ActionType::OpenApp {
            app_id: "app".into(),
            data: Some("extra".into()),
        },
        ActionType::Clipboard {
            action: ClipboardAction::Paste,
            text: None,
        },
        ActionType::DeviceControl {
            control: DeviceControl::ToggleDND(true),
        },
        ActionType::SubWorkflow {
            workflow_id: "sub".into(),
        },
    ];

    for action in &actions {
        let json = serde_json::to_string(action).unwrap();
        let back: ActionType = serde_json::from_str(&json).unwrap();
        let json2 = serde_json::to_string(&back).unwrap();
        assert_eq!(json, json2);
    }
}

#[test]
fn test_trigger_type_serialization() {
    let triggers = vec![
        TriggerType::Manual,
        TriggerType::Time {
            hour: 14,
            minute: 30,
            days_of_week: None,
        },
        TriggerType::Battery {
            level: 15,
            above: false,
        },
        TriggerType::Charging {
            state: ChargingState::Discharging,
        },
        TriggerType::WiFi {
            ssid: None,
            connected: false,
        },
        TriggerType::Bluetooth {
            device_name: None,
            connected: true,
        },
        TriggerType::DeviceState {
            state: DeviceState::Idle,
        },
        TriggerType::EventBus {
            event_name: "evt".into(),
            filter: None,
        },
    ];

    for trigger in &triggers {
        let json = serde_json::to_string(trigger).unwrap();
        let _back: TriggerType = serde_json::from_str(&json).unwrap();
    }
}

#[test]
fn test_automation_error_display() {
    let err = AutomationError::WorkflowNotFound("abc".into());
    assert_eq!(format!("{}", err), "workflow not found: abc");

    let err = AutomationError::WorkflowAlreadyExists("abc".into());
    assert_eq!(format!("{}", err), "workflow already exists: abc");

    let err = AutomationError::InvalidWorkflow("bad config".into());
    assert_eq!(format!("{}", err), "invalid workflow: bad config");

    let err = AutomationError::WorkflowDisabled("disabled-1".into());
    assert_eq!(format!("{}", err), "workflow disabled: disabled-1");

    let err = AutomationError::StepExecutionFailed {
        step: 3,
        reason: "timeout".into(),
    };
    assert_eq!(format!("{}", err), "step 3 failed: timeout");

    let err = AutomationError::ExecutionCancelled("by user".into());
    assert_eq!(format!("{}", err), "execution cancelled: by user");

    let err = AutomationError::PermissionDenied("no access".into());
    assert_eq!(format!("{}", err), "permission denied: no access");
}

#[test]
fn test_execution_record_serialization() {
    let record = ExecutionRecord {
        execution_id: "exec-1".into(),
        workflow_id: "wf-1".into(),
        workflow_name: "Test".into(),
        status: ExecutionStatus::Partial,
        started_at: 1000,
        duration_ms: 250,
        steps_succeeded: 2,
        steps_failed: 1,
        steps_total: 3,
        error: Some("something went wrong".into()),
    };

    let json = serde_json::to_string(&record).unwrap();
    let back: ExecutionRecord = serde_json::from_str(&json).unwrap();
    assert_eq!(back.execution_id, "exec-1");
    assert_eq!(back.status, ExecutionStatus::Partial);
    assert_eq!(back.error, Some("something went wrong".into()));
}

#[test]
fn test_automation_event_payload_serialization() {
    let payloads = vec![
        AutomationEventPayload::WorkflowCreated {
            workflow_id: "w1".into(),
            name: "test".into(),
        },
        AutomationEventPayload::WorkflowStarted {
            workflow_id: "w1".into(),
            execution_id: "e1".into(),
        },
        AutomationEventPayload::WorkflowCompleted {
            workflow_id: "w1".into(),
            execution_id: "e1".into(),
            steps_succeeded: 2,
            steps_failed: 0,
            duration_ms: 100,
        },
        AutomationEventPayload::WorkflowFailed {
            workflow_id: "w1".into(),
            execution_id: "e1".into(),
            error: "crash".into(),
        },
        AutomationEventPayload::TriggerActivated {
            workflow_id: "w1".into(),
            trigger_type: "manual".into(),
            reason: "user".into(),
        },
        AutomationEventPayload::ActionExecuted {
            workflow_id: "w1".into(),
            execution_id: "e1".into(),
            step: 0,
            action_type: "speak".into(),
            success: true,
        },
        AutomationEventPayload::ConditionMatched {
            workflow_id: "w1".into(),
            execution_id: "e1".into(),
            condition: "True".into(),
            matched: true,
        },
        AutomationEventPayload::AutomationError {
            workflow_id: "w1".into(),
            error: "oops".into(),
        },
    ];

    for payload in &payloads {
        let json = serde_json::to_string(payload).unwrap();
        let _back: AutomationEventPayload = serde_json::from_str(&json).unwrap();
    }
}

#[test]
fn test_workflow_summary() {
    let mut wf = Workflow::new("sum-1".into(), "Summary".into(), "test summary".into());
    wf.triggers.push(TriggerConfig {
        trigger: TriggerType::Manual,
        conditions: None,
    });
    wf.steps.push(WorkflowStep {
        id: "s1".into(),
        name: "S".into(),
        action: ActionType::Speak { text: "hi".into() },
        condition: None,
        retry_count: 0,
        timeout_ms: 5000,
        continue_on_failure: false,
    });
    wf.tags = vec!["tag1".into()];

    let summary = wf.summary();
    assert_eq!(summary.id, "sum-1");
    assert_eq!(summary.name, "Summary");
    assert_eq!(summary.step_count, 1);
    assert_eq!(summary.trigger_count, 1);
    assert_eq!(summary.state, WorkflowState::Active);
    assert_eq!(summary.tags, vec!["tag1".to_string()]);
}

#[test]
fn test_workflow_validate() {
    let wf = Workflow::new("valid".into(), "Valid".into(), "valid".into());
    // No triggers and no steps -> validation should fail
    assert!(wf.validate().is_err());

    let mut wf2 = Workflow::new("valid2".into(), "Valid2".into(), "valid2".into());
    wf2.triggers.push(TriggerConfig {
        trigger: TriggerType::Manual,
        conditions: None,
    });
    assert!(wf2.validate().is_ok());

    let mut wf3 = Workflow::new("valid3".into(), "Valid3".into(), "valid3".into());
    wf3.steps.push(WorkflowStep {
        id: "s1".into(),
        name: "S".into(),
        action: ActionType::Speak { text: "hi".into() },
        condition: None,
        retry_count: 0,
        timeout_ms: 5000,
        continue_on_failure: false,
    });
    assert!(wf3.validate().is_ok());
}
