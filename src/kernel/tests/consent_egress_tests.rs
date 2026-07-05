//! Integration tests for the Consent Gate + Egress Gate (Milestone 2).
//!
//! These exercise the public kernel API end-to-end and verify integration with the
//! global Egress Log: permission granted, denied, expired, blocked, and policy override.

use std::sync::Arc;

use nova_kernel::{
    get_recent_egress, ConsentGrant, ConsentManager, ConsentState, DestinationScope, EgressGate,
    EgressOutcome, EgressPolicy, EgressRequest, RequestKind,
};
use uuid::Uuid;

fn request(kind: RequestKind, dest: &str, purpose: &str) -> EgressRequest {
    EgressRequest {
        kind,
        destination: dest.to_string(),
        purpose: purpose.to_string(),
        data_size_bytes: 128,
        origin_module: "IntegrationTest".to_string(),
        correlation_id: Uuid::new_v4(),
    }
}

#[test]
fn granted_flow_allows_and_logs_egress() {
    let cm = Arc::new(ConsentManager::new());
    let gate = EgressGate::new(cm.clone(), EgressPolicy::InternetAllowed);
    cm.grant(
        RequestKind::Ai,
        "api.example.com",
        ConsentGrant::AlwaysAllow,
    );

    let purpose = "it_granted_flow_unique";
    let decision = gate.validate(&request(RequestKind::Ai, "api.example.com", purpose));
    assert_eq!(decision.outcome, EgressOutcome::Allowed);
    assert_eq!(decision.consent_state, ConsentState::Granted);

    // Integrated with the global egress log (D3): the decision is attributable.
    let logged = get_recent_egress()
        .into_iter()
        .rev()
        .find(|e| e.purpose == purpose)
        .expect("egress decision must be logged");
    assert!(logged.consent_granted);
    assert_eq!(logged.destination, "api.example.com");
}

#[test]
fn denied_flow_blocks_and_logs() {
    let cm = Arc::new(ConsentManager::new());
    let gate = EgressGate::new(cm, EgressPolicy::InternetAllowed);

    let purpose = "it_denied_flow_unique";
    let decision = gate.validate(&request(RequestKind::Cloud, "api.example.com", purpose));
    assert_eq!(decision.outcome, EgressOutcome::Denied);
    assert_eq!(decision.consent_state, ConsentState::RequiresPrompt);

    let logged = get_recent_egress()
        .into_iter()
        .rev()
        .find(|e| e.purpose == purpose)
        .expect("denied egress must still be logged");
    assert!(!logged.consent_granted);
}

#[test]
fn expired_once_grant_requires_prompt_again() {
    let cm = Arc::new(ConsentManager::new());
    let gate = EgressGate::new(cm.clone(), EgressPolicy::InternetAllowed);
    cm.grant(
        RequestKind::Cloud,
        "once.example.com",
        ConsentGrant::AllowOnce,
    );

    assert_eq!(
        gate.validate(&request(
            RequestKind::Cloud,
            "once.example.com",
            "it_once_1"
        ))
        .outcome,
        EgressOutcome::Allowed
    );
    let second = gate.validate(&request(
        RequestKind::Cloud,
        "once.example.com",
        "it_once_2",
    ));
    assert_eq!(second.outcome, EgressOutcome::Denied);
    assert_eq!(second.consent_state, ConsentState::RequiresPrompt);
}

#[test]
fn blocked_policy_overrides_always_allow() {
    let cm = Arc::new(ConsentManager::new());
    let gate = EgressGate::new(cm.clone(), EgressPolicy::Blocked);
    cm.grant(
        RequestKind::Ai,
        "api.example.com",
        ConsentGrant::AlwaysAllow,
    );

    let decision = gate.validate(&request(RequestKind::Ai, "api.example.com", "it_blocked"));
    assert_eq!(decision.outcome, EgressOutcome::Denied);
    assert_eq!(decision.consent_state, ConsentState::NotEvaluated);
}

#[test]
fn offline_policy_overrides_local_sync_consent() {
    let cm = Arc::new(ConsentManager::new());
    let gate = EgressGate::new(cm.clone(), EgressPolicy::OfflineOnly);
    cm.grant(RequestKind::Sync, "peer.local", ConsentGrant::AlwaysAllow);

    assert_eq!(
        EgressGate::classify("peer.local"),
        DestinationScope::LocalNetwork
    );
    let decision = gate.validate(&request(RequestKind::Sync, "peer.local", "it_offline"));
    assert_eq!(decision.outcome, EgressOutcome::Denied);
    assert_eq!(decision.consent_state, ConsentState::NotEvaluated);
}

#[test]
fn guard_enforces_deny_with_error() {
    let cm = Arc::new(ConsentManager::new());
    let gate = EgressGate::new(cm.clone(), EgressPolicy::InternetAllowed);

    // No consent -> guard returns an error the caller can propagate with `?`.
    assert!(gate
        .guard(&request(RequestKind::Ai, "api.example.com", "it_guard"))
        .is_err());

    cm.grant(
        RequestKind::Ai,
        "api.example.com",
        ConsentGrant::AlwaysAllow,
    );
    assert!(gate
        .guard(&request(RequestKind::Ai, "api.example.com", "it_guard"))
        .is_ok());
}
