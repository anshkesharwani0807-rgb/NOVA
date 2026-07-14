//! Consent-gated remote inference provider (Milestone 6, FR-AI-004).
//!
//! Implements the **acceleration seam**: an optional, opt-in backend that routes
//! inference requests to a remote API when (a) the user has explicitly enabled it
//! and (b) the [`EgressGate`] authorises the outbound call.
//!
//! # Privacy guarantees
//! - **Disabled by default.** The provider returns `ERR_AI_REMOTE_DISABLED` unless
//!   the caller explicitly enables it.
//! - **Every call goes through [`EgressGate::guard`].** If the gate denies (offline
//!   policy, no consent grant, or always-deny), the inference errors out immediately
//!   with the gate's reason — no data leaves the device.
//! - **Every attempt is logged** by the gate into the Activity Trail and Egress Log
//!   (D3, ADR-0009), whether allowed or denied.
//! - **No fallback.** A denied request errors; it never silently falls back to the
//!   local model. The caller decides what to do (e.g. retry locally).
//!
//! # Wiring (composition root)
//! ```ignore
//! let remote = Arc::new(
//!     RemoteProvider::new("cloud-accel", kernel.egress_gate.clone())
//!         .with_endpoint("https://api.example.com/v1/chat")
//! );
//! ai_engine.register_provider(remote)?;
//! ```
//!
//! # Simulation mode (tests / demo)
//! When compiled without real HTTP, `RemoteProvider` returns a deterministic simulated
//! response so the gate logic is fully exercisable in tests without a network.

use crate::provider::{
    Cancellation, ChunkSink, FinishReason, InferenceChunk, InferenceProvider, InferenceRequest,
    Message, ModelDescriptor, Role,
};
use async_trait::async_trait;
use nova_kernel::{EgressGate, EgressRequest, ErrorCategory, NovaError, RequestKind, Result};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use uuid::Uuid;

/// An inference provider that routes requests to a remote API via the Egress Gate.
///
/// Disabled by default; must be explicitly enabled by the user (FR-AI-004).
pub struct RemoteProvider {
    id: String,
    /// The remote API endpoint (e.g. `https://api.example.com/v1/chat`).
    endpoint: String,
    /// The gate through which all outbound calls must pass.
    egress: Arc<EgressGate>,
    /// Master enable switch. False by default — user must opt in.
    enabled: AtomicBool,
    /// Simulated response used when real HTTP is unavailable (tests / demo).
    sim_response: Option<String>,
}

impl RemoteProvider {
    /// Create a disabled remote provider.
    ///
    /// * `id`     — Stable model identifier (registered with `ModelManager`).
    /// * `egress` — The kernel's egress gate; cloned from `kernel.egress_gate`.
    pub fn new(id: impl Into<String>, egress: Arc<EgressGate>) -> Self {
        Self {
            id: id.into(),
            endpoint: String::new(),
            egress,
            enabled: AtomicBool::new(false),
            sim_response: None,
        }
    }

    /// Set the remote API endpoint.
    pub fn with_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = endpoint.into();
        self
    }

    /// Install a simulated response (used in tests / demo instead of real HTTP).
    pub fn with_sim_response(mut self, response: impl Into<String>) -> Self {
        self.sim_response = Some(response.into());
        self
    }

    /// Enable the acceleration seam (user opt-in). Should only be called after the
    /// user has explicitly consented *and* the Egress Gate policy allows internet.
    pub fn enable(&self) {
        self.enabled.store(true, Ordering::SeqCst);
        tracing::info!(
            "Remote inference provider '{}' enabled (acceleration seam active).",
            self.id
        );
    }

    /// Disable the acceleration seam — reverts to local-only immediately.
    pub fn disable(&self) {
        self.enabled.store(false, Ordering::SeqCst);
        tracing::info!(
            "Remote inference provider '{}' disabled (local-only mode).",
            self.id
        );
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::SeqCst)
    }

    /// Format messages into a plain-text prompt for the simulated path.
    fn format_prompt(messages: &[Message]) -> String {
        messages
            .iter()
            .map(|m| {
                let role = match m.role {
                    Role::System => "System",
                    Role::User => "User",
                    Role::Assistant => "Assistant",
                    Role::Tool => "Tool",
                };
                format!("{}: {}", role, m.content)
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[async_trait]
impl InferenceProvider for RemoteProvider {
    fn id(&self) -> &str {
        &self.id
    }

    fn describe(&self) -> ModelDescriptor {
        ModelDescriptor {
            id: self.id.clone(),
            provider: "remote".to_string(),
            context_window: 128_000, // frontier models typically have large context
            local: false,
            loaded: self.is_enabled(),
        }
    }

    fn is_loaded(&self) -> bool {
        // A remote provider is "loaded" when it is enabled and has a valid endpoint.
        self.is_enabled() && !self.endpoint.is_empty()
    }

    async fn infer(
        &self,
        req: &InferenceRequest,
        cancel: &Cancellation,
        sink: &ChunkSink,
    ) -> Result<()> {
        // Gate 1: master enable switch (user opt-in, FR-AI-004).
        if !self.is_enabled() {
            return Err(NovaError::new(
                ErrorCategory::EgressDenied,
                "ERR_AI_REMOTE_DISABLED",
                "remote acceleration seam is disabled; enable it in settings to use cloud inference",
            ));
        }

        if self.endpoint.is_empty() {
            return Err(NovaError::new(
                ErrorCategory::Internal,
                "ERR_AI_REMOTE_NO_ENDPOINT",
                "no remote endpoint configured",
            ));
        }

        if cancel.is_cancelled() {
            sink.emit(InferenceChunk::Done {
                finish_reason: FinishReason::Cancelled,
            });
            return Ok(());
        }

        let correlation_id = Uuid::new_v4();
        let prompt = Self::format_prompt(&req.messages);

        // Gate 2: Egress Gate (D3). This call logs the attempt regardless of outcome.
        // If denied, `guard` returns Err — no data is sent.
        let _decision = self.egress.guard(&EgressRequest {
            kind: RequestKind::Ai,
            destination: self.endpoint.clone(),
            purpose: "nova.ai.remote_inference".to_string(),
            // Approximate payload size in bytes.
            data_size_bytes: prompt.len(),
            origin_module: "ai".to_string(),
            correlation_id,
        })?; // propagates EgressDenied error to the caller

        tracing::info!(
            "Remote inference request to '{}' authorised by egress gate (correlation={})",
            self.endpoint,
            correlation_id
        );

        // --- Outbound inference ---
        // The acceleration seam is gated end-to-end by `EgressGate` above. The actual
        // transport is supplied by the composition root: when a real client is wired in
        // it performs the POST here; otherwise a configured `sim_response` exercises the
        // full gate → infer → stream → outcome path offline (FR-AI-004 testing support).
        // If neither is configured we fail honestly rather than emit fabricated output.
        let response_text = match &self.sim_response {
            Some(sim) => sim.clone(),
            None => {
                return Err(NovaError::new(
                    ErrorCategory::Internal,
                    "ERR_AI_REMOTE_NO_CLIENT",
                    "no remote transport configured (set a sim_response for tests or wire a real client in the composition root)",
                ));
            }
        };

        // Stream the response word-by-word (consistent with other providers).
        for word in response_text.split_inclusive(' ') {
            if cancel.is_cancelled() {
                sink.emit(InferenceChunk::Done {
                    finish_reason: FinishReason::Cancelled,
                });
                return Ok(());
            }
            sink.emit(InferenceChunk::Token(word.to_string()));
        }

        sink.emit(InferenceChunk::Done {
            finish_reason: FinishReason::Stop,
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{Accumulator, ChunkSink};
    use crate::provider::{InferenceParams, InferenceRequest, Message};
    use nova_kernel::{ConsentGrant, ConsentManager, EgressGate, EgressPolicy};
    use parking_lot::Mutex;
    use std::sync::Arc;
    use tokio::sync::mpsc;

    fn make_gate(policy: EgressPolicy) -> Arc<EgressGate> {
        let cm = Arc::new(ConsentManager::new());
        Arc::new(EgressGate::new(cm, policy))
    }

    fn make_gate_with_consent(policy: EgressPolicy, endpoint: &str) -> Arc<EgressGate> {
        let cm = Arc::new(ConsentManager::new());
        cm.grant(RequestKind::Ai, endpoint, ConsentGrant::AlwaysAllow);
        Arc::new(EgressGate::new(cm, policy))
    }

    fn simple_req() -> InferenceRequest {
        InferenceRequest::new(
            vec![Message::user("Hello, remote!")],
            InferenceParams::default(),
        )
    }

    fn sink_and_acc() -> (ChunkSink, Arc<Mutex<Accumulator>>) {
        let (tx, _rx) = mpsc::unbounded_channel();
        let acc = Arc::new(Mutex::new(Accumulator::default()));
        (ChunkSink::new(tx, acc.clone()), acc)
    }

    #[tokio::test]
    async fn disabled_by_default_returns_error() {
        let provider = RemoteProvider::new("remote", make_gate(EgressPolicy::InternetAllowed))
            .with_endpoint("https://api.example.com/v1")
            .with_sim_response("Hello from cloud.");
        let (sink, _acc) = sink_and_acc();
        let err = provider
            .infer(&simple_req(), &Cancellation::new(), &sink)
            .await
            .unwrap_err();
        assert_eq!(err.code, "ERR_AI_REMOTE_DISABLED");
    }

    #[tokio::test]
    async fn offline_policy_blocks_even_when_enabled() {
        let provider = RemoteProvider::new("remote", make_gate(EgressPolicy::OfflineOnly))
            .with_endpoint("https://api.example.com/v1")
            .with_sim_response("Hello from cloud.");
        provider.enable();
        let (sink, _acc) = sink_and_acc();
        let err = provider
            .infer(&simple_req(), &Cancellation::new(), &sink)
            .await
            .unwrap_err();
        // Gate must deny — EgressDenied category.
        assert_eq!(err.category, ErrorCategory::EgressDenied);
    }

    #[tokio::test]
    async fn no_consent_blocks_even_when_enabled() {
        let provider = RemoteProvider::new("remote", make_gate(EgressPolicy::InternetAllowed))
            .with_endpoint("https://api.example.com/v1")
            .with_sim_response("Hello from cloud.");
        provider.enable();
        let (sink, _acc) = sink_and_acc();
        let err = provider
            .infer(&simple_req(), &Cancellation::new(), &sink)
            .await
            .unwrap_err();
        assert_eq!(err.category, ErrorCategory::EgressDenied);
    }

    #[tokio::test]
    async fn enabled_with_consent_streams_response() {
        let endpoint = "https://api.example.com/v1";
        let provider = RemoteProvider::new(
            "remote",
            make_gate_with_consent(EgressPolicy::InternetAllowed, endpoint),
        )
        .with_endpoint(endpoint)
        .with_sim_response("Hello from cloud.");
        provider.enable();

        let (tx, mut rx) = mpsc::unbounded_channel();
        let acc = Arc::new(Mutex::new(Accumulator::default()));
        let sink = ChunkSink::new(tx, acc.clone());

        provider
            .infer(&simple_req(), &Cancellation::new(), &sink)
            .await
            .unwrap();

        // Drain the stream and check we got tokens + Done.
        let mut text = String::new();
        let mut got_done = false;
        while let Ok(chunk) = rx.try_recv() {
            match chunk {
                InferenceChunk::Token(t) => text.push_str(&t),
                InferenceChunk::Done { .. } => got_done = true,
                _ => {}
            }
        }
        assert!(got_done);
        assert!(text.contains("Hello from cloud"));
    }

    #[tokio::test]
    async fn cancellation_stops_stream() {
        let endpoint = "https://api.example.com/v1";
        let provider = RemoteProvider::new(
            "remote",
            make_gate_with_consent(EgressPolicy::InternetAllowed, endpoint),
        )
        .with_endpoint(endpoint)
        .with_sim_response("word1 word2 word3 word4 word5");
        provider.enable();

        let cancel = Cancellation::new();
        cancel.cancel(); // cancel immediately before infer

        let (sink, _acc) = sink_and_acc();
        provider.infer(&simple_req(), &cancel, &sink).await.unwrap();
        // No panic/error — cancelled gracefully.
    }

    #[tokio::test]
    async fn disable_reverts_to_local_immediately() {
        let endpoint = "https://api.example.com/v1";
        let provider = RemoteProvider::new(
            "remote",
            make_gate_with_consent(EgressPolicy::InternetAllowed, endpoint),
        )
        .with_endpoint(endpoint)
        .with_sim_response("Hello from cloud.");
        provider.enable();
        assert!(provider.is_enabled());
        provider.disable();
        assert!(!provider.is_enabled());

        let (sink, _acc) = sink_and_acc();
        let err = provider
            .infer(&simple_req(), &Cancellation::new(), &sink)
            .await
            .unwrap_err();
        assert_eq!(err.code, "ERR_AI_REMOTE_DISABLED");
    }

    // ── FR-AI-004: audit logging ───────────────────────────────────────────────

    #[tokio::test]
    async fn allowed_remote_inference_is_logged_to_egress_ledger() {
        let endpoint = "https://api.example.com/v1";
        let gate = make_gate_with_consent(EgressPolicy::InternetAllowed, endpoint);
        let provider = RemoteProvider::new("remote", gate.clone())
            .with_endpoint(endpoint)
            .with_sim_response("Hello from cloud.");
        provider.enable();

        let before = gate.recent_decisions().len();
        let (_sink, _acc) = sink_and_acc();
        // Drive infer through the same sink path used above.
        let (tx, _rx) = mpsc::unbounded_channel();
        let acc = Arc::new(Mutex::new(Accumulator::default()));
        let sink = ChunkSink::new(tx, acc);
        provider
            .infer(&simple_req(), &Cancellation::new(), &sink)
            .await
            .unwrap();

        let decisions = gate.recent_decisions();
        assert_eq!(
            decisions.len(),
            before + 1,
            "exactly one egress decision logged"
        );
        let d = decisions.last().unwrap();
        assert!(d.allowed(), "decision should be allowed: {:?}", d);
        assert_eq!(d.destination, endpoint);
        assert_eq!(d.kind, RequestKind::Ai);
    }

    #[tokio::test]
    async fn denied_remote_inference_is_logged_to_egress_ledger() {
        let endpoint = "https://api.example.com/v1";
        let gate = make_gate(EgressPolicy::OfflineOnly);
        let provider = RemoteProvider::new("remote", gate.clone())
            .with_endpoint(endpoint)
            .with_sim_response("Hello from cloud.");
        provider.enable();

        let before = gate.recent_decisions().len();
        let (_sink, _acc) = sink_and_acc();
        let (tx, _rx) = mpsc::unbounded_channel();
        let acc = Arc::new(Mutex::new(Accumulator::default()));
        let sink = ChunkSink::new(tx, acc);
        let err = provider
            .infer(&simple_req(), &Cancellation::new(), &sink)
            .await
            .unwrap_err();
        assert_eq!(err.category, ErrorCategory::EgressDenied);

        let decisions = gate.recent_decisions();
        assert_eq!(decisions.len(), before + 1, "denial is still logged");
        assert!(!decisions.last().unwrap().allowed());
    }

    #[tokio::test]
    async fn disabled_provider_logs_no_egress_decision() {
        // When the seam is disabled, no outbound attempt is made and nothing reaches the
        // egress gate — so no egress decision is recorded (no queued/leaked calls).
        let endpoint = "https://api.example.com/v1";
        let gate = make_gate_with_consent(EgressPolicy::InternetAllowed, endpoint);
        let provider = RemoteProvider::new("remote", gate.clone())
            .with_endpoint(endpoint)
            .with_sim_response("Hello from cloud.");
        // NOT enabled.

        let before = gate.recent_decisions().len();
        let (_sink, _acc) = sink_and_acc();
        let (tx, _rx) = mpsc::unbounded_channel();
        let acc = Arc::new(Mutex::new(Accumulator::default()));
        let sink = ChunkSink::new(tx, acc);
        let err = provider
            .infer(&simple_req(), &Cancellation::new(), &sink)
            .await
            .unwrap_err();
        assert_eq!(err.code, "ERR_AI_REMOTE_DISABLED");

        assert_eq!(
            gate.recent_decisions().len(),
            before,
            "disabled seam must not reach the egress gate"
        );
    }
}
