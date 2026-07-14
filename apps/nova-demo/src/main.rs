//! NOVA developer demo — a runnable end-to-end smoke test of the kernel.
//!
//! This is NOT the product UI (there is no app/screen yet). It exists so a developer
//! can *see* the kernel work: it boots, starts a few module skeletons, drives one
//! pub/sub event and one request/response through the Event Bus, and prints the
//! privacy-first config defaults plus the user-facing activity and egress trails.
//!
//! Run it with:  cargo run -p nova_demo

use std::sync::Arc;

use nova_ai::{AIEngine, RemoteProvider, DEFAULT_SESSION};
use nova_comms::DeviceComms;
use nova_kernel::{
    get_config, get_recent_activity, get_recent_egress, ConsentGrant, EgressPolicy, EgressRequest,
    EventMetadata, Kernel, NovaEvent, RequestKind,
};
use nova_memory::{MemoryCategory, MemoryEngine, MemoryRecord, Query, SortBy};
use nova_plugin_host::PluginHost;
use nova_search::UniversalSearch;
use nova_voice::VoiceSystem;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("========================================");
    println!(" NOVA demo — kernel smoke test");
    println!("========================================\n");

    // 1) Runtime dirs live on the project's own drive (never C:), under a gitignored
    //    `.nova-runtime/` folder. Derived from the crate location so it works from any cwd.
    let project_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent() // apps/nova-demo -> apps
        .and_then(std::path::Path::parent) // apps -> project root
        .expect("crate is nested two levels under the project root");
    let base = project_root.join(".nova-runtime");
    let config_dir = base.join("config");
    let log_dir = base.join("logs");
    std::fs::create_dir_all(&config_dir)?;
    std::fs::create_dir_all(&log_dir)?;

    // 2) Boot the kernel (initializes logging, loads config, creates the event bus).
    println!("[1] Bootstrapping kernel...");
    let kernel = Kernel::bootstrap(&config_dir, &log_dir)?;

    // 3) Show the privacy-first defaults are actually in force (Principles 2 & 6).
    let cfg = get_config();
    println!(
        "    privacy.local_by_default      = {}",
        cfg.privacy.local_by_default
    );
    println!(
        "    privacy.allow_remote_accel    = {}",
        cfg.privacy.allow_remote_acceleration
    );
    println!(
        "    privacy.telemetry_enabled     = {}",
        cfg.privacy.telemetry_enabled
    );
    println!(
        "    automation.autonomy_level     = {}",
        cfg.automation.autonomy_level
    );

    // 4) Register all modules with the kernel registry and bring them up through the
    //    lifecycle manager in dependency order (Milestone 3).
    println!("\n[2] Registering modules + lifecycle (Milestone 3)...");
    // Keep a handle to the Memory Engine to demonstrate its API (Milestone 4); register
    // the same instance so the registry drives its lifecycle (opens the database).
    let memory = Arc::new(MemoryEngine::new(kernel.clone()));
    kernel.registry.register(memory.clone())?;
    kernel
        .registry
        .register(Arc::new(UniversalSearch::new(kernel.clone())))?;
    let ai = Arc::new(AIEngine::new(kernel.clone()));
    kernel.registry.register(ai.clone())?;
    kernel
        .registry
        .register(Arc::new(VoiceSystem::new(kernel.clone())))?;
    kernel
        .registry
        .register(Arc::new(DeviceComms::new(kernel.clone())))?;
    kernel
        .registry
        .register(Arc::new(PluginHost::new(kernel.clone())))?;
    println!("    registered {} modules", kernel.registry.count());
    kernel.registry.bring_up().await?;
    for m in kernel.registry.list() {
        println!(
            "    - {:<12} v{:<6} {:?}  health={:?}",
            m.id, m.version, m.state, m.health.status
        );
    }

    // 4b) Encrypted Memory Engine (Milestone 4) — persistent, offline, encrypted store.
    println!("\n[2b] Memory Engine (Milestone 4 — encrypted SQLite):");
    println!("     db: {}", memory.db_path().display());
    // Start fresh each run so the demo is deterministic.
    for r in memory.find(&Query::new().include_deleted(true))? {
        memory.purge(&r.id)?;
    }
    // Store a few memories.
    let note = MemoryRecord::new(
        MemoryCategory::Knowledge,
        "Coast trip",
        "Sunset photos, 2019",
    )
    .with_tags(["photos", "travel"])
    .with_importance(70);
    let note_id = note.id.clone();
    memory.insert(&note)?;
    memory.insert(
        &MemoryRecord::new(MemoryCategory::Reminder, "Passport", "Renew before August")
            .with_tags(["travel"]),
    )?;
    println!("     stored {} memories", memory.total()?);

    // Simulate an application restart: close and reopen the database.
    memory.close();
    memory.open()?;
    println!(
        "     after restart, loaded {} memories from disk",
        memory.total()?
    );

    // Search (semantic-free local search over the encrypted store).
    let hits = memory.search(&Query::new().contains("photos").sort(SortBy::ImportanceDesc))?;
    println!(
        "     search 'photos' -> {} hit(s): {:?}",
        hits.len(),
        hits.first().map(|r| &r.title)
    );

    // Update, soft-delete, then restore.
    let mut updated = memory.find_by_id(&note_id)?.expect("note present");
    updated.content = "Sunset photos, coast trip, 2019".to_string();
    memory.update(&updated)?;
    memory.delete(&note_id)?;
    println!(
        "     after soft-delete, active memories = {}",
        memory.count(&Query::new())?
    );
    memory.restore_record(&note_id)?;
    println!(
        "     after restore, active memories = {}",
        memory.count(&Query::new())?
    );

    // Health report from the module.
    println!(
        "     health[memory] = {} records (encrypted at rest)",
        memory.total()?
    );

    // 5) Publish a pub/sub event (e.g. a user capturing a note).
    println!("\n[3] Publishing a capture event...");
    let meta = EventMetadata::new("DemoShell", Some("user_capture".to_string()));
    let payload: Arc<String> = Arc::new("Note: buy milk".to_string());
    let subscribers = kernel.event_bus.publish(NovaEvent {
        metadata: meta,
        payload,
    })?;
    println!("    delivered to {subscribers} subscriber(s)");

    // 6) Drive a request/response through Universal Search (skeleton reply expected).
    println!("\n[4] Sending a search query (skeleton handler)...");
    let smeta = EventMetadata::new("DemoShell", Some("search".to_string()));
    let query = nova_search::SearchQuery::partial("birthday photos 2019").limit(10);
    let response = kernel
        .event_bus
        .request("search:query", smeta, Arc::new(query))
        .await?;
    let body = response
        .payload
        .downcast_ref::<Vec<nova_search::SearchResult>>()
        .map(|results| format!("{} results", results.len()))
        .unwrap_or_else(|| "<non-text response>".to_string());
    println!("    search response: {body}");

    // 6b) AI Runtime (Milestone 6 — offline-first inference + uncertainty surfacing).
    println!("\n[4b] AI Runtime (Milestone 6 — local inference):");
    // Drive a turn through the public API. The runtime streams internally and returns an
    // outcome that carries an estimated confidence (Principle 9 — honesty about limits /
    // FR-AI-003), so uncertainty is surfaced rather than hidden behind a confident tone.
    let handle = ai
        .complete(DEFAULT_SESSION, "What did I store about the coast trip?")
        .await?;
    let outcome = handle.finish().await?;
    println!("     reply      : {}", outcome.text);
    println!(
        "     confidence : {:.2} (uncertainty flagged: {})",
        outcome.confidence, outcome.uncertainty_flagged
    );

    // The same runtime is reachable through the event bus (ai:inference request handler),
    // proving the AI module integrates with the kernel's pub/sub + request/response seams.
    let aimeta = EventMetadata::new("DemoShell", Some("ai_inference".to_string()));
    let ai_response = kernel
        .event_bus
        .request("ai:inference", aimeta, Arc::new("Hello NOVA".to_string()))
        .await?;
    let ai_text = ai_response
        .payload
        .downcast_ref::<String>()
        .cloned()
        .unwrap_or_default();
    println!("     via bus    : {ai_text}");

    // Let the spawned async listeners flush their log lines.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // 7) Show the user-facing Activity Trail (Principle 5 — transparency).
    println!("\n[5] Activity trail (what NOVA did, and why):");
    for entry in get_recent_activity().iter().rev().take(6).rev() {
        println!(
            "    - {} :: {} — {}",
            entry.module, entry.action, entry.reason
        );
    }

    // 8) Show the Egress Log (D3 — proof that nothing left the device).
    let egress = get_recent_egress();
    println!("\n[6] Egress log (D3 — network activity):");
    if egress.is_empty() {
        println!("    (empty — nothing left the device, exactly as expected offline)");
    } else {
        for e in &egress {
            println!(
                "    - {} -> {} (consent={})",
                e.purpose, e.destination, e.consent_granted
            );
        }
    }

    // 9) Egress Gate (Milestone 2) — every outbound request is validated first.
    println!("\n[7] Egress Gate (Milestone 2):");
    println!("    active policy: {:?}", kernel.egress_gate.policy());
    let outbound = EgressRequest {
        kind: RequestKind::Ai,
        destination: "api.example.com".to_string(),
        purpose: "demo_ai_inference".to_string(),
        data_size_bytes: 256,
        origin_module: "DemoShell".to_string(),
        correlation_id: EventMetadata::new("DemoShell", None).correlation_id,
    };
    let d1 = kernel.egress_gate.validate(&outbound);
    println!(
        "    AI -> api.example.com  : {:?} — {}",
        d1.outcome, d1.reason
    );

    println!("    (user consents + enables the acceleration seam...)");
    kernel.consent.grant(
        RequestKind::Ai,
        "api.example.com",
        ConsentGrant::AlwaysAllow,
    );
    kernel.egress_gate.set_policy(EgressPolicy::InternetAllowed);
    let mut retry = outbound.clone();
    retry.correlation_id = EventMetadata::new("DemoShell", None).correlation_id;
    let d2 = kernel.egress_gate.validate(&retry);
    println!(
        "    AI -> api.example.com  : {:?} — {}",
        d2.outcome, d2.reason
    );

    // 7b) Remote acceleration seam (FR-AI-004) — disabled by default, egress-gated.
    println!("\n[7b] Remote acceleration seam (FR-AI-004):");
    let remote = Arc::new(
        RemoteProvider::new("cloud-accel", kernel.egress_gate.clone())
            .with_endpoint("https://api.example.com/v1/chat")
            .with_sim_response("NOVA cloud: higher-capability reply (simulated)."),
    );
    ai.register_provider(remote.clone())?;
    ai.models().set_active("cloud-accel")?;

    // Disabled by default: routing to the seam refuses rather than leaking data.
    let disabled_reply = match ai.chat("remote-demo", "hello from cloud?").await {
        Ok(t) => t,
        Err(e) => format!("refused: {e}"),
    };
    println!("     disabled    : {disabled_reply}");

    // Grant consent for the exact endpoint (internet policy already set in [7]), then enable.
    kernel.consent.grant(
        RequestKind::Ai,
        "https://api.example.com/v1/chat",
        ConsentGrant::AlwaysAllow,
    );
    remote.enable();
    let enabled_reply = ai.chat("remote-demo", "hello from cloud?").await?;
    println!("     enabled     : {enabled_reply}");

    // Disabling reverts to local-only immediately (no queued outbound calls).
    remote.disable();
    let reverted = match ai.chat("remote-demo", "hello from cloud?").await {
        Ok(t) => t,
        Err(e) => format!("refused: {e}"),
    };
    println!("     re-disabled : {reverted}");

    // Restore the local mock as the active model for a clean post-condition.
    ai.models().set_active("mock-local").ok();

    // 10) Tear down modules in reverse dependency order (Milestone 3), then the kernel.
    println!("\n[8] Shutting down modules (reverse order)...");
    kernel.registry.tear_down().await?;
    for m in kernel.registry.list() {
        println!("    - {:<12} {:?}", m.id, m.state);
    }

    kernel.shutdown();
    println!("\n========================================");
    println!(" Demo complete. Foundation + gates + module lifecycle + offline AI inference work.");
    println!("========================================");
    Ok(())
}
