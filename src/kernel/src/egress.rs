//! Kernel-level Egress Gate (Milestone 2).
//!
//! ALL outbound interactions — network, plugin, AI, sync, cloud, external — must pass
//! through this gate before execution (D3, ADR-0003). The gate applies the active
//! [`EgressPolicy`], then consults the [`ConsentManager`], logs every decision to the
//! Activity Trail and the Egress Log (ADR-0009), and returns an allow/deny decision.
//!
//! Default posture is conservative: policy is checked first and can deny regardless of
//! consent (policy override), and an absent consent grant denies until approved.

use crate::consent::{ConsentManager, ConsentResolution, ConsentState, RequestKind};
use crate::error::{ErrorCategory, NovaError, Result};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

/// The network reach permitted by the gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EgressPolicy {
    /// No network egress at all (fully offline).
    OfflineOnly,
    /// Only local-network destinations (LAN / mDNS / loopback); internet blocked.
    LocalNetworkOnly,
    /// Internet destinations allowed (still subject to consent).
    InternetAllowed,
    /// Everything blocked, unconditionally (overrides consent).
    Blocked,
}

/// Classification of a destination's network scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DestinationScope {
    LocalNetwork,
    Internet,
}

/// Allow/deny outcome of an egress evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EgressOutcome {
    Allowed,
    Denied,
}

/// A request to perform an outbound interaction, submitted to the gate.
#[derive(Debug, Clone)]
pub struct EgressRequest {
    pub kind: RequestKind,
    pub destination: String,
    pub purpose: String,
    pub data_size_bytes: usize,
    pub origin_module: String,
    pub correlation_id: Uuid,
}

/// The gate's decision, recorded in its ledger and forwarded to the global egress log.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EgressDecision {
    pub outcome: EgressOutcome,
    pub reason: String,
    pub consent_state: ConsentState,
    pub scope: DestinationScope,
    pub kind: RequestKind,
    pub destination: String,
    pub timestamp: String,
    pub correlation_id: Uuid,
}

impl EgressDecision {
    /// True if this decision permits the outbound interaction.
    pub fn allowed(&self) -> bool {
        matches!(self.outcome, EgressOutcome::Allowed)
    }
}

/// The single chokepoint through which all outbound interactions must pass (D3).
pub struct EgressGate {
    policy: RwLock<EgressPolicy>,
    consent: Arc<ConsentManager>,
    ledger: Mutex<Vec<EgressDecision>>,
}

impl EgressGate {
    pub fn new(consent: Arc<ConsentManager>, policy: EgressPolicy) -> Self {
        Self {
            policy: RwLock::new(policy),
            consent,
            ledger: Mutex::new(Vec::new()),
        }
    }

    /// The active egress policy.
    pub fn policy(&self) -> EgressPolicy {
        *self.policy.read()
    }

    /// Change the active egress policy (e.g. the user opts into the acceleration seam).
    pub fn set_policy(&self, policy: EgressPolicy) {
        *self.policy.write() = policy;
    }

    /// Access the consent manager backing this gate.
    pub fn consent(&self) -> &Arc<ConsentManager> {
        &self.consent
    }

    /// Classify a destination string as local-network or internet scope.
    ///
    /// Deterministic and offline (no DNS): loopback, private IPv4 ranges, and `.local`
    /// mDNS names are local; everything else is internet.
    pub fn classify(destination: &str) -> DestinationScope {
        let mut host = destination.trim();
        for prefix in ["https://", "http://", "wss://", "ws://"] {
            if let Some(rest) = host.strip_prefix(prefix) {
                host = rest;
                break;
            }
        }
        host = host.split('/').next().unwrap_or(host);
        let lower = host.to_ascii_lowercase();
        if lower == "::1" || lower == "[::1]" {
            return DestinationScope::LocalNetwork;
        }
        let hostname = if let Some(stripped) = lower.strip_prefix('[') {
            stripped.split(']').next().unwrap_or(&lower).to_string()
        } else if lower.matches(':').count() == 1 {
            lower.split(':').next().unwrap_or(&lower).to_string()
        } else {
            lower.clone()
        };
        if hostname == "localhost" || hostname == "127.0.0.1" || hostname.ends_with(".local") {
            return DestinationScope::LocalNetwork;
        }
        if hostname.starts_with("10.") || hostname.starts_with("192.168.") {
            return DestinationScope::LocalNetwork;
        }
        if hostname.starts_with("172.") {
            if let Some(octet) = hostname
                .split('.')
                .nth(1)
                .and_then(|x| x.parse::<u8>().ok())
            {
                if (16..=31).contains(&octet) {
                    return DestinationScope::LocalNetwork;
                }
            }
        }
        DestinationScope::Internet
    }

    /// Validate an outbound request. Always returns a decision and always logs it.
    pub fn validate(&self, req: &EgressRequest) -> EgressDecision {
        let scope = Self::classify(&req.destination);
        let policy = *self.policy.read();

        let (outcome, consent_state, reason) = match policy {
            EgressPolicy::Blocked => (
                EgressOutcome::Denied,
                ConsentState::NotEvaluated,
                "blocked by policy: Blocked".to_string(),
            ),
            EgressPolicy::OfflineOnly => (
                EgressOutcome::Denied,
                ConsentState::NotEvaluated,
                "blocked by policy: OfflineOnly (no network egress)".to_string(),
            ),
            EgressPolicy::LocalNetworkOnly if scope == DestinationScope::Internet => (
                EgressOutcome::Denied,
                ConsentState::NotEvaluated,
                "blocked by policy: LocalNetworkOnly (internet destination)".to_string(),
            ),
            EgressPolicy::LocalNetworkOnly | EgressPolicy::InternetAllowed => {
                match self.consent.authorize(req.kind, &req.destination) {
                    ConsentResolution::Granted(source) => (
                        EgressOutcome::Allowed,
                        ConsentState::Granted,
                        format!("allowed ({})", source.label()),
                    ),
                    ConsentResolution::Denied => (
                        EgressOutcome::Denied,
                        ConsentState::Denied,
                        "denied by user (always-deny)".to_string(),
                    ),
                    ConsentResolution::RequiresPrompt => (
                        EgressOutcome::Denied,
                        ConsentState::RequiresPrompt,
                        "consent required (no grant on record)".to_string(),
                    ),
                }
            }
        };

        let decision = EgressDecision {
            outcome,
            reason,
            consent_state,
            scope,
            kind: req.kind,
            destination: req.destination.clone(),
            timestamp: chrono::Local::now().to_rfc3339(),
            correlation_id: req.correlation_id,
        };

        // Integrate with the existing Activity Trail (Principle 5) and Egress Log (D3):
        // every decision is logged with timestamp, destination, reason, consent state,
        // and correlation id.
        crate::logger::log_activity(
            &req.origin_module,
            &format!("egress:{:?}", req.kind),
            &decision.reason,
            Some(req.correlation_id),
        );
        crate::logger::log_egress(
            &req.destination,
            &req.purpose,
            req.data_size_bytes,
            decision.allowed(),
            Some(req.correlation_id),
        );

        let mut ledger = self.ledger.lock().unwrap();
        ledger.push(decision.clone());
        if ledger.len() > 10_000 {
            ledger.remove(0);
        }
        decision
    }

    /// Validate and enforce: `Ok(decision)` if allowed, `Err(EgressDenied)` otherwise.
    /// Modules MUST call this with `?` before performing any outbound interaction.
    pub fn guard(&self, req: &EgressRequest) -> Result<EgressDecision> {
        let decision = self.validate(req);
        if decision.allowed() {
            Ok(decision)
        } else {
            Err(NovaError::new(
                ErrorCategory::EgressDenied,
                "ERR_EGRESS_DENIED",
                &decision.reason,
            )
            .with_correlation(req.correlation_id))
        }
    }

    /// Snapshot of recent egress decisions for inspection or tests.
    pub fn recent_decisions(&self) -> Vec<EgressDecision> {
        self.ledger.lock().unwrap().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::consent::ConsentGrant;

    fn gate(policy: EgressPolicy) -> (Arc<ConsentManager>, EgressGate) {
        let cm = Arc::new(ConsentManager::new());
        (cm.clone(), EgressGate::new(cm, policy))
    }

    fn req(kind: RequestKind, dest: &str) -> EgressRequest {
        EgressRequest {
            kind,
            destination: dest.to_string(),
            purpose: "unit_test".to_string(),
            data_size_bytes: 0,
            origin_module: "test".to_string(),
            correlation_id: Uuid::new_v4(),
        }
    }

    #[test]
    fn classify_local_vs_internet() {
        for d in [
            "localhost",
            "127.0.0.1",
            "192.168.1.5",
            "10.0.0.2",
            "172.16.0.1",
            "printer.local",
            "::1",
            "https://peer.local/path",
        ] {
            assert_eq!(
                EgressGate::classify(d),
                DestinationScope::LocalNetwork,
                "{d} should be local"
            );
        }
        for d in [
            "example.com",
            "8.8.8.8",
            "172.32.0.1",
            "https://api.example.com/v1",
        ] {
            assert_eq!(
                EgressGate::classify(d),
                DestinationScope::Internet,
                "{d} should be internet"
            );
        }
    }

    #[test]
    fn blocked_policy_overrides_consent() {
        let (cm, g) = gate(EgressPolicy::Blocked);
        cm.grant(
            RequestKind::Ai,
            "api.example.com",
            ConsentGrant::AlwaysAllow,
        );
        let d = g.validate(&req(RequestKind::Ai, "api.example.com"));
        assert_eq!(d.outcome, EgressOutcome::Denied);
        assert_eq!(d.consent_state, ConsentState::NotEvaluated);
    }

    #[test]
    fn offline_only_denies_all() {
        let (_cm, g) = gate(EgressPolicy::OfflineOnly);
        assert_eq!(
            g.validate(&req(RequestKind::Sync, "peer.local")).outcome,
            EgressOutcome::Denied
        );
        assert_eq!(
            g.validate(&req(RequestKind::Ai, "example.com")).outcome,
            EgressOutcome::Denied
        );
    }

    #[test]
    fn local_network_only_allows_local_denies_internet() {
        let (cm, g) = gate(EgressPolicy::LocalNetworkOnly);
        cm.grant(RequestKind::Sync, "peer.local", ConsentGrant::AlwaysAllow);
        assert_eq!(
            g.validate(&req(RequestKind::Sync, "peer.local")).outcome,
            EgressOutcome::Allowed
        );
        cm.grant(RequestKind::Ai, "example.com", ConsentGrant::AlwaysAllow);
        assert_eq!(
            g.validate(&req(RequestKind::Ai, "example.com")).outcome,
            EgressOutcome::Denied
        );
    }

    #[test]
    fn internet_allowed_requires_consent() {
        let (cm, g) = gate(EgressPolicy::InternetAllowed);
        let d1 = g.validate(&req(RequestKind::Ai, "api.example.com"));
        assert_eq!(d1.outcome, EgressOutcome::Denied);
        assert_eq!(d1.consent_state, ConsentState::RequiresPrompt);
        cm.grant(
            RequestKind::Ai,
            "api.example.com",
            ConsentGrant::AlwaysAllow,
        );
        assert_eq!(
            g.validate(&req(RequestKind::Ai, "api.example.com")).outcome,
            EgressOutcome::Allowed
        );
    }

    #[test]
    fn allow_once_then_expires_via_gate() {
        let (cm, g) = gate(EgressPolicy::InternetAllowed);
        cm.grant(
            RequestKind::Cloud,
            "api.example.com",
            ConsentGrant::AllowOnce,
        );
        assert_eq!(
            g.validate(&req(RequestKind::Cloud, "api.example.com"))
                .outcome,
            EgressOutcome::Allowed
        );
        let d = g.validate(&req(RequestKind::Cloud, "api.example.com"));
        assert_eq!(d.outcome, EgressOutcome::Denied);
        assert_eq!(d.consent_state, ConsentState::RequiresPrompt);
    }

    #[test]
    fn always_deny_blocks() {
        let (cm, g) = gate(EgressPolicy::InternetAllowed);
        cm.grant(
            RequestKind::External,
            "bad.example.com",
            ConsentGrant::AlwaysDeny,
        );
        let d = g.validate(&req(RequestKind::External, "bad.example.com"));
        assert_eq!(d.outcome, EgressOutcome::Denied);
        assert_eq!(d.consent_state, ConsentState::Denied);
    }

    #[test]
    fn guard_errors_on_deny_and_ok_on_allow() {
        let (cm, g) = gate(EgressPolicy::InternetAllowed);
        assert!(g.guard(&req(RequestKind::Ai, "api.example.com")).is_err());
        cm.grant(
            RequestKind::Ai,
            "api.example.com",
            ConsentGrant::AlwaysAllow,
        );
        assert!(g.guard(&req(RequestKind::Ai, "api.example.com")).is_ok());
    }

    #[test]
    fn ledger_records_every_decision() {
        let (_cm, g) = gate(EgressPolicy::Blocked);
        g.validate(&req(RequestKind::Network, "example.com"));
        g.validate(&req(RequestKind::Ai, "example.com"));
        assert_eq!(g.recent_decisions().len(), 2);
    }

    #[test]
    fn set_policy_changes_behavior() {
        let (cm, g) = gate(EgressPolicy::OfflineOnly);
        cm.grant(
            RequestKind::Ai,
            "api.example.com",
            ConsentGrant::AlwaysAllow,
        );
        assert_eq!(
            g.validate(&req(RequestKind::Ai, "api.example.com")).outcome,
            EgressOutcome::Denied
        );
        g.set_policy(EgressPolicy::InternetAllowed);
        assert_eq!(
            g.validate(&req(RequestKind::Ai, "api.example.com")).outcome,
            EgressOutcome::Allowed
        );
    }
}
