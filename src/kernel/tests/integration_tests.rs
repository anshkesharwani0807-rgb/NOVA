use nova_kernel::{
    get_recent_activity, get_recent_egress, log_activity, log_egress, EventMetadata, NovaConfig,
    NovaEvent, NovaResponse,
};
use std::sync::Arc;

// ─────────────────────────────────────────────────────────────────────────────
// Config validation — pure unit tests, no kernel bootstrap needed
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_config_invalid_autonomy_level() {
    let mut cfg = NovaConfig::default();
    cfg.automation.autonomy_level = "dangerous".to_string();
    assert!(
        cfg.validate().is_err(),
        "Invalid autonomy level should fail validation"
    );
}

#[test]
fn test_config_invalid_device_tier() {
    let mut cfg = NovaConfig::default();
    cfg.system.device_tier = "supercomputer".to_string();
    assert!(
        cfg.validate().is_err(),
        "Invalid device tier should fail validation"
    );
}

#[test]
fn test_config_valid_defaults() {
    let cfg = NovaConfig::default();
    assert!(
        cfg.validate().is_ok(),
        "Default config must pass validation"
    );
    // Principle 2 — privacy defaults must be conservative out of the box
    assert!(
        cfg.privacy.local_by_default,
        "local_by_default must be true"
    );
    assert!(
        !cfg.privacy.allow_remote_acceleration,
        "remote acceleration must default to off"
    );
    assert!(
        !cfg.privacy.telemetry_enabled,
        "telemetry must default to off"
    );
    // Principle 6 — autonomy dial must default conservative (D8)
    assert_eq!(cfg.automation.autonomy_level, "conservative");
    assert!(cfg.automation.require_consent_for_destructive);
}

// ─────────────────────────────────────────────────────────────────────────────
// Logging — activity trail & egress log
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_activity_logging() {
    let id = uuid::Uuid::new_v4();
    log_activity("TestModule", "test_action", "unit test purpose", Some(id));
    let trail = get_recent_activity();
    let last = trail
        .last()
        .expect("Activity trail should not be empty after logging");
    assert_eq!(last.module, "TestModule");
    assert_eq!(last.action, "test_action");
    assert_eq!(last.correlation_id, Some(id));
}

#[test]
fn test_egress_logging() {
    log_egress("api.example.com", "unit_test_egress", 512, true, None);
    let log = get_recent_egress();
    let last = log
        .last()
        .expect("Egress log should not be empty after logging");
    assert_eq!(last.destination, "api.example.com");
    assert_eq!(last.purpose, "unit_test_egress");
    assert_eq!(last.data_size_bytes, 512);
    assert!(last.consent_granted);
}

// ─────────────────────────────────────────────────────────────────────────────
// Event Bus — standalone tests (no kernel bootstrap, no singleton involved)
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_event_bus_pubsub() {
    use nova_kernel::EventBus;
    let bus = EventBus::new(64);
    let mut rx = bus.subscribe();

    let meta = EventMetadata::new("Sender", Some("ping".to_string()));
    let payload: Arc<String> = Arc::new("hello".to_string());
    let event = NovaEvent {
        metadata: meta.clone(),
        payload,
    };

    bus.publish(event).unwrap();

    let received = rx.recv().await.unwrap();
    assert_eq!(received.metadata.origin_module, "Sender");
    assert_eq!(received.metadata.correlation_id, meta.correlation_id);
    let data = received.payload.downcast_ref::<String>().unwrap();
    assert_eq!(data, "hello");
}

#[tokio::test]
async fn test_event_bus_request_response_echo() {
    use nova_kernel::EventBus;
    let bus = Arc::new(EventBus::new(64));
    let bus2 = bus.clone();

    let mut req_rx = bus.register_request_handler("svc:echo", 8).unwrap();

    tokio::spawn(async move {
        if let Some(req) = req_rx.recv().await {
            let echo: Arc<String> = req.payload.downcast::<String>().unwrap();
            let res_meta = EventMetadata::child_of(&req.metadata, "EchoSvc", None);
            let _ = req.response_tx.send(Ok(NovaResponse {
                metadata: res_meta,
                payload: echo,
            }));
        }
    });

    let meta = EventMetadata::new("Client", Some("echo_call".to_string()));
    let payload: Arc<String> = Arc::new("echo_payload".to_string());
    let resp = bus2.request("svc:echo", meta, payload).await.unwrap();

    let body = resp.payload.downcast_ref::<String>().unwrap();
    assert_eq!(body, "echo_payload");
    assert_eq!(resp.metadata.origin_module, "EchoSvc");
}

#[tokio::test]
async fn test_event_bus_unknown_handler_errors() {
    use nova_kernel::EventBus;
    let bus = EventBus::new(64);
    let meta = EventMetadata::new("Client", None);
    let payload: Arc<String> = Arc::new("data".to_string());
    let result = bus.request("svc:nonexistent", meta, payload).await;
    assert!(
        result.is_err(),
        "Requesting an unregistered handler must return an error"
    );
}

#[test]
fn test_event_bus_duplicate_handler_registration_errors() {
    use nova_kernel::EventBus;
    let bus = EventBus::new(64);
    let _first = bus.register_request_handler("svc:exclusive", 4).unwrap();
    let second = bus.register_request_handler("svc:exclusive", 4);
    assert!(
        second.is_err(),
        "Registering the same handler name twice must return an error"
    );
}
