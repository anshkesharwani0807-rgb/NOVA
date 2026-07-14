//! NFR-PERF-002 — AI inference performance benchmarks (run as regression tests).
//!
//! NFR-PERF-002 targets the end-of-speech → response-onset latency at
//! `< 1.5 s` (hard limit `3 s`) on minimum hardware, offline. These benchmarks measure
//! the local, offline path for:
//!   1. single-turn inference latency,
//!   2. generation throughput (tokens/second),
//!   3. cold (model load) vs warm (already loaded) startup, and
//!   4. resident memory growth during model registration + inference.
//!
//! Each measurement is asserted against a generous bound so the suite acts as a
//! regression gate: if a change makes local inference pathologically slow or leaky,
//! the build/test step fails. Absolute numbers are printed for visibility.

use async_trait::async_trait;
use nova_ai::provider::{
    Cancellation, ChunkSink, FinishReason, InferenceChunk, InferenceProvider, InferenceRequest,
    Message, ModelDescriptor,
};
use nova_ai::{InferenceEngine, InferenceParams, ModelManager, ToolRegistry};
use nova_kernel::{Kernel, Result};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use uuid::Uuid;

// ── Helpers ───────────────────────────────────────────────────────────────────

fn test_kernel() -> Arc<Kernel> {
    let base = std::env::temp_dir().join(format!("nova-perf-{}", Uuid::new_v4()));
    let config_dir = base.join("config");
    let log_dir = base.join("logs");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::create_dir_all(&log_dir).unwrap();
    Kernel::bootstrap(&config_dir, &log_dir).unwrap_or_else(|_| Kernel::instance().unwrap())
}

/// Resident set size of the current process in bytes (best-effort, cross-platform).
fn resident_set_bytes() -> u64 {
    use sysinfo::{Pid, ProcessRefreshKind, RefreshKind, System};
    let mut sys = System::new_with_specifics(
        RefreshKind::new().with_processes(ProcessRefreshKind::everything()),
    );
    sys.refresh_processes_specifics(ProcessRefreshKind::everything());
    sys.process(Pid::from_u32(std::process::id()))
        .map(|p| p.memory())
        .unwrap_or(0)
}

/// A provider that streams a fixed number of tokens with no delay (throughput probe).
struct ManyTokens {
    id: String,
    tokens: usize,
    loaded: AtomicBool,
}

#[async_trait]
impl InferenceProvider for ManyTokens {
    fn id(&self) -> &str {
        &self.id
    }
    fn describe(&self) -> ModelDescriptor {
        ModelDescriptor {
            id: self.id.clone(),
            provider: "bench".into(),
            context_window: 2048,
            local: true,
            loaded: self.is_loaded(),
        }
    }
    fn is_loaded(&self) -> bool {
        self.loaded.load(Ordering::SeqCst)
    }
    async fn load(&self) -> Result<()> {
        self.loaded.store(true, Ordering::SeqCst);
        Ok(())
    }
    async fn unload(&self) -> Result<()> {
        self.loaded.store(false, Ordering::SeqCst);
        Ok(())
    }
    async fn infer(
        &self,
        _req: &InferenceRequest,
        cancel: &Cancellation,
        sink: &ChunkSink,
    ) -> Result<()> {
        for i in 0..self.tokens {
            if cancel.is_cancelled() {
                sink.emit(InferenceChunk::Done {
                    finish_reason: FinishReason::Cancelled,
                });
                return Ok(());
            }
            sink.emit(InferenceChunk::Token(format!("t{i} ")));
        }
        sink.emit(InferenceChunk::Done {
            finish_reason: FinishReason::Stop,
        });
        Ok(())
    }
}

/// A provider whose `load()` is deliberately slow so cold vs warm startup is observable.
struct SlowLoad {
    id: String,
    load_ms: u64,
    loaded: AtomicBool,
}

#[async_trait]
impl InferenceProvider for SlowLoad {
    fn id(&self) -> &str {
        &self.id
    }
    fn describe(&self) -> ModelDescriptor {
        ModelDescriptor {
            id: self.id.clone(),
            provider: "bench".into(),
            context_window: 2048,
            local: true,
            loaded: self.is_loaded(),
        }
    }
    fn is_loaded(&self) -> bool {
        self.loaded.load(Ordering::SeqCst)
    }
    async fn load(&self) -> Result<()> {
        tokio::time::sleep(std::time::Duration::from_millis(self.load_ms)).await;
        self.loaded.store(true, Ordering::SeqCst);
        Ok(())
    }
    async fn unload(&self) -> Result<()> {
        self.loaded.store(false, Ordering::SeqCst);
        Ok(())
    }
    async fn infer(
        &self,
        _req: &InferenceRequest,
        cancel: &Cancellation,
        sink: &ChunkSink,
    ) -> Result<()> {
        for w in ["cold ", "vs ", "warm ", "reply "] {
            if cancel.is_cancelled() {
                sink.emit(InferenceChunk::Done {
                    finish_reason: FinishReason::Cancelled,
                });
                return Ok(());
            }
            sink.emit(InferenceChunk::Token(w.to_string()));
        }
        sink.emit(InferenceChunk::Done {
            finish_reason: FinishReason::Stop,
        });
        Ok(())
    }
}

fn mk_engine(
    provider: Arc<dyn InferenceProvider>,
) -> (Arc<Kernel>, Arc<InferenceEngine>, Arc<ModelManager>) {
    let kernel = test_kernel();
    let models = Arc::new(ModelManager::new());
    models.register(provider).unwrap();
    let engine = Arc::new(InferenceEngine::new(
        kernel.clone(),
        models.clone(),
        Arc::new(ToolRegistry::new()),
    ));
    (kernel, engine, models)
}

fn req(text: &str) -> InferenceRequest {
    InferenceRequest::new(vec![Message::user(text)], InferenceParams::default())
}

// ── 1. Inference latency ──────────────────────────────────────────────────────

#[tokio::test]
async fn inference_latency_within_nfr_perf_002() {
    let (_k, engine, _models) = mk_engine(Arc::new(ManyTokens {
        id: "many".into(),
        tokens: 50,
        loaded: AtomicBool::new(false),
    }));

    // Warm-up (pays any lazy load), then measure a representative turn.
    let _ = engine.infer_collect(req("warmup"), Uuid::new_v4()).await;

    let start = Instant::now();
    let outcome = engine
        .infer_collect(req("hello"), Uuid::new_v4())
        .await
        .unwrap();
    let elapsed_ms = start.elapsed().as_millis();

    println!(
        "[NFR-PERF-002] inference latency = {elapsed_ms} ms (target < 1500 ms, hard < 3000 ms)"
    );
    assert!(outcome.finish_reason == FinishReason::Stop);
    assert!(elapsed_ms < 3000, "latency exceeds hard limit of 3 s");
}

// ── 2. Throughput ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn generation_throughput_is_positive() {
    let n_tokens = 500u64;
    let (_k, engine, _models) = mk_engine(Arc::new(ManyTokens {
        id: "many".into(),
        tokens: n_tokens as usize,
        loaded: AtomicBool::new(false),
    }));

    let _ = engine.infer_collect(req("warmup"), Uuid::new_v4()).await;

    let start = Instant::now();
    let outcome = engine
        .infer_collect(req("stream"), Uuid::new_v4())
        .await
        .unwrap();
    let elapsed_s = start.elapsed().as_secs_f64().max(1e-6);
    let tok_s = (n_tokens as f64) / elapsed_s;

    println!("[NFR-PERF-002] throughput = {tok_s:.1} tokens/s over {n_tokens} tokens");
    assert!(outcome.text.split_whitespace().count() as u64 >= n_tokens);
    assert!(tok_s > 50.0, "throughput implausibly low: {tok_s:.1} tok/s");
}

// ── 3. Cold vs warm startup ───────────────────────────────────────────────────

#[tokio::test]
async fn cold_startup_is_slower_than_warm() {
    let (_k, engine, _models) = mk_engine(Arc::new(SlowLoad {
        id: "slow".into(),
        load_ms: 60,
        loaded: AtomicBool::new(false),
    }));

    // Cold: first inference triggers the (slow) lazy model load.
    let cold_start = Instant::now();
    engine
        .infer_collect(req("cold"), Uuid::new_v4())
        .await
        .unwrap();
    let cold_ms = cold_start.elapsed().as_millis();

    // Warm: model already in memory, no load cost.
    let warm_start = Instant::now();
    engine
        .infer_collect(req("warm"), Uuid::new_v4())
        .await
        .unwrap();
    let warm_ms = warm_start.elapsed().as_millis();

    println!("[NFR-PERF-002] cold = {cold_ms} ms, warm = {warm_ms} ms");
    assert!(cold_ms >= warm_ms, "cold startup should include load cost");
    assert!(warm_ms < 3000, "warm latency exceeds hard limit");
    assert!(cold_ms < 3000, "cold latency exceeds hard limit");
}

// ── 4. Memory usage ───────────────────────────────────────────────────────────

#[tokio::test]
async fn memory_usage_stays_within_budget() {
    let before = resident_set_bytes();

    let (_k, engine, models) = mk_engine(Arc::new(ManyTokens {
        id: "many".into(),
        tokens: 50,
        loaded: AtomicBool::new(false),
    }));

    // Register a few extra providers and run several inferences to exercise the runtime.
    for i in 0..4 {
        let _ = models.register(Arc::new(ManyTokens {
            id: format!("extra-{i}"),
            tokens: 20,
            loaded: AtomicBool::new(false),
        }));
    }
    for i in 0..8 {
        let _ = engine
            .infer_collect(req(&format!("turn {i}")), Uuid::new_v4())
            .await;
    }
    for i in 0..8 {
        let _ = engine
            .infer_collect(req(&format!("turn {i}")), Uuid::new_v4())
            .await;
    }

    let after = resident_set_bytes();
    let delta_mb = (after.saturating_sub(before)) as f64 / (1024.0 * 1024.0);

    println!(
        "[NFR-PERF-002] resident memory delta = {delta_mb:.1} MB over 5 providers + 8 inferences"
    );
    // Generous smoke bound: the offline mock runtime must not balloon memory.
    // Real-model peaks are governed by NFR-RES-004 (< 2 GB); this harness asserts the
    // runtime plumbing itself is bounded.
    assert!(
        delta_mb < 256.0,
        "resident memory growth too high: {delta_mb:.1} MB"
    );
}
