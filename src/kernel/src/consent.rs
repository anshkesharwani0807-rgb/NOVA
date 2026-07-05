//! Kernel-level Consent Manager (Milestone 2).
//!
//! Records and evaluates user consent for external interactions. Works with the Egress
//! Gate (see [`crate::egress`]) to enforce Principle 2 (privacy by default) and
//! Principle 6 / D8 (agency with consent). The posture is conservative: absent an
//! explicit grant, the answer is "ask the user" ([`ConsentResolution::RequiresPrompt`]),
//! which the Egress Gate treats as deny-until-approved.
//!
//! Storage is in-memory for Milestone 2. Durable persistence of the always-allow /
//! always-deny decisions across restarts is deferred to Chapter 14 (Database).

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// The kind of external interaction that must be validated before execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RequestKind {
    Network,
    Plugin,
    Ai,
    Sync,
    Cloud,
    External,
}

/// A consent grant a user can give for a `(kind, destination)` pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConsentGrant {
    /// Valid for exactly one authorized request, then expires.
    AllowOnce,
    /// Valid until the session is reset (e.g. app restart).
    AllowForSession,
    /// Persistent allow until explicitly revoked.
    AlwaysAllow,
    /// Persistent deny until explicitly revoked.
    AlwaysDeny,
}

/// Where a granted authorization came from (used for transparent reasons).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GrantSource {
    Always,
    Session,
    Once,
}

impl GrantSource {
    /// A short human-readable label for the activity trail.
    pub fn label(self) -> &'static str {
        match self {
            GrantSource::Always => "always-allow",
            GrantSource::Session => "session grant",
            GrantSource::Once => "one-time grant",
        }
    }
}

/// The result of authorizing a request against recorded consent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsentResolution {
    /// Authorized by an existing grant (with its source).
    Granted(GrantSource),
    /// Explicitly denied by an always-deny decision.
    Denied,
    /// No decision on record — the user must be prompted.
    RequiresPrompt,
}

/// Consent state recorded on an egress decision (for the audit trail).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConsentState {
    Granted,
    Denied,
    RequiresPrompt,
    /// Consent was not consulted because policy blocked the request first.
    NotEvaluated,
}

fn key(kind: RequestKind, destination: &str) -> String {
    format!("{:?}|{}", kind, destination.trim().to_ascii_lowercase())
}

/// Records and evaluates user consent decisions.
#[derive(Default)]
pub struct ConsentManager {
    /// AlwaysAllow / AlwaysDeny decisions (persistent within the process).
    persistent: RwLock<HashMap<String, ConsentGrant>>,
    /// AllowForSession keys, cleared on [`ConsentManager::reset_session`].
    session: RwLock<HashSet<String>>,
    /// AllowOnce remaining counts, decremented on authorization.
    once: RwLock<HashMap<String, u32>>,
}

impl ConsentManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a consent grant for a `(kind, destination)` pair.
    pub fn grant(&self, kind: RequestKind, destination: &str, grant: ConsentGrant) {
        let k = key(kind, destination);
        match grant {
            ConsentGrant::AlwaysAllow | ConsentGrant::AlwaysDeny => {
                self.persistent.write().insert(k, grant);
            }
            ConsentGrant::AllowForSession => {
                self.session.write().insert(k);
            }
            ConsentGrant::AllowOnce => {
                *self.once.write().entry(k).or_insert(0) += 1;
            }
        }
    }

    /// Revoke any recorded consent for a `(kind, destination)` pair.
    pub fn revoke(&self, kind: RequestKind, destination: &str) {
        let k = key(kind, destination);
        self.persistent.write().remove(&k);
        self.session.write().remove(&k);
        self.once.write().remove(&k);
    }

    /// Reset the session, expiring all AllowForSession grants.
    pub fn reset_session(&self) {
        self.session.write().clear();
    }

    /// Non-consuming inspection of the current consent state.
    pub fn state(&self, kind: RequestKind, destination: &str) -> ConsentState {
        let k = key(kind, destination);
        if let Some(grant) = self.persistent.read().get(&k) {
            return match grant {
                ConsentGrant::AlwaysDeny => ConsentState::Denied,
                _ => ConsentState::Granted,
            };
        }
        if self.session.read().contains(&k) {
            return ConsentState::Granted;
        }
        if self.once.read().get(&k).copied().unwrap_or(0) > 0 {
            return ConsentState::Granted;
        }
        ConsentState::RequiresPrompt
    }

    /// Authorize a request, consuming a one-time grant if that is what authorizes it.
    ///
    /// Resolution order: persistent (always) → session → one-time → requires prompt.
    pub fn authorize(&self, kind: RequestKind, destination: &str) -> ConsentResolution {
        let k = key(kind, destination);
        if let Some(grant) = self.persistent.read().get(&k) {
            return match grant {
                ConsentGrant::AlwaysAllow => ConsentResolution::Granted(GrantSource::Always),
                ConsentGrant::AlwaysDeny => ConsentResolution::Denied,
                // AllowOnce / AllowForSession are never stored in `persistent`.
                _ => ConsentResolution::RequiresPrompt,
            };
        }
        if self.session.read().contains(&k) {
            return ConsentResolution::Granted(GrantSource::Session);
        }
        let mut once = self.once.write();
        if let Some(remaining) = once.get_mut(&k) {
            if *remaining > 0 {
                *remaining -= 1;
                if *remaining == 0 {
                    once.remove(&k);
                }
                return ConsentResolution::Granted(GrantSource::Once);
            }
        }
        ConsentResolution::RequiresPrompt
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn always_allow_and_deny_persist() {
        let cm = ConsentManager::new();
        cm.grant(
            RequestKind::Ai,
            "api.example.com",
            ConsentGrant::AlwaysAllow,
        );
        cm.grant(
            RequestKind::Cloud,
            "sync.example.com",
            ConsentGrant::AlwaysDeny,
        );
        assert!(matches!(
            cm.authorize(RequestKind::Ai, "api.example.com"),
            ConsentResolution::Granted(GrantSource::Always)
        ));
        assert_eq!(
            cm.authorize(RequestKind::Cloud, "sync.example.com"),
            ConsentResolution::Denied
        );
    }

    #[test]
    fn allow_once_expires_after_use() {
        let cm = ConsentManager::new();
        cm.grant(RequestKind::Network, "host", ConsentGrant::AllowOnce);
        assert!(matches!(
            cm.authorize(RequestKind::Network, "host"),
            ConsentResolution::Granted(GrantSource::Once)
        ));
        assert_eq!(
            cm.authorize(RequestKind::Network, "host"),
            ConsentResolution::RequiresPrompt
        );
    }

    #[test]
    fn session_grant_expires_on_reset() {
        let cm = ConsentManager::new();
        cm.grant(
            RequestKind::Sync,
            "peer.local",
            ConsentGrant::AllowForSession,
        );
        assert!(matches!(
            cm.authorize(RequestKind::Sync, "peer.local"),
            ConsentResolution::Granted(GrantSource::Session)
        ));
        cm.reset_session();
        assert_eq!(
            cm.authorize(RequestKind::Sync, "peer.local"),
            ConsentResolution::RequiresPrompt
        );
    }

    #[test]
    fn unknown_requires_prompt() {
        let cm = ConsentManager::new();
        assert_eq!(
            cm.authorize(RequestKind::External, "unknown"),
            ConsentResolution::RequiresPrompt
        );
    }

    #[test]
    fn revoke_clears_grant() {
        let cm = ConsentManager::new();
        cm.grant(RequestKind::Ai, "x", ConsentGrant::AlwaysAllow);
        cm.revoke(RequestKind::Ai, "x");
        assert_eq!(
            cm.authorize(RequestKind::Ai, "x"),
            ConsentResolution::RequiresPrompt
        );
    }

    #[test]
    fn key_is_case_insensitive() {
        let cm = ConsentManager::new();
        cm.grant(
            RequestKind::Ai,
            "API.Example.COM",
            ConsentGrant::AlwaysAllow,
        );
        assert!(matches!(
            cm.authorize(RequestKind::Ai, "api.example.com"),
            ConsentResolution::Granted(_)
        ));
    }

    #[test]
    fn state_is_non_consuming() {
        let cm = ConsentManager::new();
        cm.grant(RequestKind::Network, "host", ConsentGrant::AllowOnce);
        assert_eq!(
            cm.state(RequestKind::Network, "host"),
            ConsentState::Granted
        );
        // state() did not consume the one-time grant.
        assert!(matches!(
            cm.authorize(RequestKind::Network, "host"),
            ConsentResolution::Granted(GrantSource::Once)
        ));
    }
}
