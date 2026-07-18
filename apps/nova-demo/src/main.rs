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
use nova_automation::{ActionType, AutomationEngine, Scheduler, TriggerConfig, TriggerType};
use nova_comms::DeviceComms;
use nova_cross_device::{
    AndroidAdapter, CommandTarget, CrossDeviceCoordinator, DeviceManager, SessionManager,
    UnifiedCommandIntent, WindowsAdapter,
};
use nova_input::InputSystem;
use nova_kernel::{
    get_config, get_recent_activity, get_recent_egress, ConsentGrant, EgressPolicy, EgressRequest,
    EventMetadata, Kernel, KernelModule, NovaEvent, RequestKind,
};
use nova_knowledge::{EntitySource, EntityType, KnowledgeEngine};
use nova_memory::{MemoryCategory, MemoryEngine, MemoryRecord, Query, SortBy};
use nova_pairing::PairingManager;
use nova_plugin_host::PluginHost;
use nova_plugin_sdk::{Plugin, PluginContext, PluginManager, PluginManifest};
use nova_screen::ScreenSystem;
use nova_search::UniversalSearch;
use nova_security::{PermissionManager, SecurityManager};
use nova_sync::SyncManager;
use nova_transport::{TransportConfig, TransportManager};
use nova_voice::VoiceSystem;
use nova_windows_agent::WindowsAgent;
use parking_lot::RwLock;

// ── Sample plugins for M13 Plugin SDK Demo ──────────────────────────────────
use async_trait::async_trait;

struct HelloPlugin {
    manifest: PluginManifest,
}

#[async_trait]
impl Plugin for HelloPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn on_enable(&self, ctx: &PluginContext) -> nova_kernel::Result<()> {
        ctx.log("HelloPlugin enabled — Hello from plugin system!");
        Ok(())
    }
}

struct MemoryPlugin {
    manifest: PluginManifest,
}

#[async_trait]
impl Plugin for MemoryPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn on_enable(&self, ctx: &PluginContext) -> nova_kernel::Result<()> {
        ctx.storage.store("last_access", "enabled");
        ctx.log("MemoryPlugin enabled — can read memories");
        Ok(())
    }
}

struct AutomationPlugin {
    manifest: PluginManifest,
}

#[async_trait]
impl Plugin for AutomationPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn on_enable(&self, ctx: &PluginContext) -> nova_kernel::Result<()> {
        ctx.storage.store("trigger_count", "0");
        ctx.log("AutomationPlugin enabled — can execute workflows");
        Ok(())
    }
}
// ── End sample plugins ──────────────────────────────────────────────────────

fn decision_label(d: &nova_automation::ConsentDecision) -> &'static str {
    match d {
        nova_automation::ConsentDecision::Allowed => "allowed",
        nova_automation::ConsentDecision::Blocked { .. } => "blocked",
        nova_automation::ConsentDecision::RequiresPrompt { .. } => "requires prompt",
    }
}

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
    let voice = Arc::new(VoiceSystem::new(kernel.clone()));
    kernel.registry.register(voice.clone())?;
    kernel
        .registry
        .register(Arc::new(DeviceComms::new(kernel.clone())))?;
    kernel
        .registry
        .register(Arc::new(PluginHost::new(kernel.clone())))?;
    // InputSystem and ScreenSystem (Milestone 18 — computer control).
    let input = Arc::new(InputSystem::new());
    input.set_event_bus(kernel.event_bus.clone());
    kernel.registry.register(input.clone())?;
    let screen = Arc::new(ScreenSystem::new(kernel.clone()));
    kernel.registry.register(screen.clone())?;
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

    // 6c) Voice System (Milestone 7 — offline-first voice pipeline).
    println!("\n[4c] Voice System (Milestone 7 — offline pipeline: wake → ASR → AI → TTS):");
    // The pipeline was started during module bring-up and runs the scripted offline source
    // through the provider stack (mock capture/VAD/wake/ASR/TTS) into the AI Runtime over the
    // bus — no microphone, no network. Give it a moment to finish the scripted turn.
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    if let Some(session) = voice.session_manager() {
        let snap = session.snapshot();
        println!("     wake words detected : {}", snap.counters.wake_words);
        println!(
            "     commands recognized : {}",
            snap.counters.commands_recognized
        );
        println!(
            "     responses spoken    : {}",
            snap.counters.responses_spoken
        );
        println!("     interruptions       : {}", snap.counters.interruptions);
        println!(
            "     recognition failures: {}",
            snap.counters.recognition_failures
        );
        println!("     (all processing stayed on-device; see the activity trail for each event)");
    }

    // 6d) Knowledge & Memory Intelligence (Milestone 11 — offline, privacy-first).
    println!("\n[4d] Knowledge Engine (Milestone 11 — memory analysis + graph + timeline):");
    let knowledge = Arc::new(KnowledgeEngine::new());
    knowledge.set_memory(memory.clone());
    knowledge.set_search(Arc::new(UniversalSearch::new(kernel.clone())));
    // Analyze the memories we inserted earlier.
    for r in memory.find(&nova_memory::Query::new().include_deleted(true))? {
        let analyzed = knowledge.analyze_memory(&r)?;
        println!(
            "     analyzed  [{}]  category={}  importance={}  tags={:?}",
            analyzed.memory_id, analyzed.category, analyzed.importance, analyzed.tags
        );
    }
    // Generate a timeline (daily).
    let all_memories = memory.find(&nova_memory::Query::new().include_deleted(true))?;
    match knowledge.generate_timeline(&all_memories, "daily") {
        Ok(tl) => println!(
            "     timeline  {}  entries={}  range={:?}",
            tl.granularity,
            tl.entries.len(),
            tl.time_range
        ),
        Err(e) => println!("     timeline  error: {e}"),
    }
    // Show the knowledge graph.
    {
        let graph = knowledge.get_graph();
        println!(
            "     entities={}  relationships={}",
            graph.entity_count(),
            graph.relationship_count()
        );
        if graph.entity_count() > 0 {
            for e in graph.all_entities() {
                println!("       entity   {} ({})", e.name, e.entity_type);
            }
        }
    }
    // Generate a summary.
    match knowledge.summarize(&all_memories, "cluster", "today") {
        Ok(s) => println!(
            "     summary   type={}  len={}",
            s.summary_type,
            s.content.len()
        ),
        Err(e) => println!("     summary   error: {e}"),
    }
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

    // 7c) Automation Engine (Milestone 12 — workflow, triggers, scheduler, execution, history).
    println!("\n[7c] Automation Engine (Milestone 12 — workflows + triggers + execution):");
    let engine = AutomationEngine::new();
    engine.set_event_bus(kernel.event_bus.clone());
    // Create and register a workflow with a manual trigger.
    let mut wf = engine.create_workflow("demo_reminder", "Remind me about something");
    wf.triggers.push(TriggerConfig {
        trigger: TriggerType::Manual,
        conditions: None,
    });
    wf.steps.push(nova_automation::WorkflowStep {
        id: "step1".into(),
        name: "Notify user".into(),
        action: ActionType::Notify {
            title: "Automation Demo".into(),
            body: "Hello from automation!".into(),
            priority: nova_automation::NotifyPriority::Normal,
        },
        condition: None,
        retry_count: 0,
        timeout_ms: 30_000,
        continue_on_failure: false,
    });
    engine.register_workflow(wf.clone())?;
    println!(
        "     registered workflow: {} (enabled={})",
        wf.name, wf.enabled
    );
    // Trigger the workflow manually.
    let execution_id = engine.trigger_manual(&wf.id)?;
    println!("     triggered execution: {}", execution_id);
    // Check execution history.
    let records = engine.history().by_workflow(&wf.id, 10);
    println!(
        "     history entries    : {} for workflow '{}'",
        records.len(),
        wf.name
    );
    if let Some(last) = records.first() {
        println!("     last status        : {:?}", last.status);
    }
    // Demonstrate scheduler trigger checking.
    let cfg = nova_automation::AutomationConfig::default();
    let scheduler = Scheduler::new(cfg);
    let workflows = engine.registry().all();
    let ctx = std::collections::HashMap::<String, String>::new();
    let triggered = scheduler.check_triggers(&workflows, &ctx);
    println!(
        "     scheduler triggered: {} of {} workflow(s)",
        triggered.len(),
        workflows.len()
    );
    // Demonstrate event bus integration.
    let mut rx = kernel.event_bus.subscribe();
    let mut wf2 = engine.create_workflow("event_demo", "Event-driven workflow");
    wf2.triggers.push(TriggerConfig {
        trigger: TriggerType::Manual,
        conditions: None,
    });
    wf2.steps.push(nova_automation::WorkflowStep {
        id: "step2".into(),
        name: "Speak".into(),
        action: ActionType::Speak {
            text: "Event bus works!".into(),
        },
        condition: None,
        retry_count: 0,
        timeout_ms: 30_000,
        continue_on_failure: false,
    });
    engine.register_workflow(wf2)?;
    let _ = engine.trigger_manual("event_demo");
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let mut capturable_events = 0;
    loop {
        match rx.try_recv() {
            Ok(ev) if ev.metadata.origin_module == "automation" => capturable_events += 1,
            Err(_) => break,
            _ => {}
        }
    }
    println!(
        "     event bus events   : {} automation event(s) captured",
        capturable_events
    );
    // Clean up the execution state.
    let _ = engine.cancel_execution(&execution_id);

    // 7d) Plugin SDK (Milestone 13 — plugin lifecycle, permissions, sandbox, storage, events).
    println!("\n[7d] Plugin SDK (Milestone 13 — plugins + permissions + sandbox + storage):");
    let plugin_mgr = PluginManager::new(Some(kernel.event_bus.clone()));

    // Register HelloPlugin.
    let hello = Arc::new(HelloPlugin {
        manifest: PluginManifest::new(
            "hello",
            "Hello Plugin",
            "1.0.0",
            "NOVA",
            "A friendly plugin",
        ),
    });
    plugin_mgr.register_plugin(hello).unwrap();
    plugin_mgr.install_plugin("hello").await.unwrap();
    plugin_mgr.enable_plugin("hello").await.unwrap();
    println!(
        "     hello plugin  : installed, enabled, health={}",
        plugin_mgr.check_health("hello").unwrap()
    );

    // Register MemoryPlugin with memory permissions.
    let mem = Arc::new(MemoryPlugin {
        manifest: PluginManifest::new(
            "memory_reader",
            "Memory Reader",
            "1.0.0",
            "NOVA",
            "Reads memories",
        )
        .with_permissions(&["memory.read", "memory.write"]),
    });
    plugin_mgr.register_plugin(mem).unwrap();
    plugin_mgr.install_plugin("memory_reader").await.unwrap();
    plugin_mgr.enable_plugin("memory_reader").await.unwrap();
    // Check permission enforcement.
    let perm_ok = plugin_mgr
        .check_action("memory_reader", "read", "memory.read")
        .is_ok();
    let perm_denied = plugin_mgr
        .check_action("memory_reader", "network", "internet.access")
        .is_err();
    println!(
        "     memory plugin  : permitted(memory.read)={}, blocked(internet.access)={}",
        perm_ok, perm_denied
    );

    // Register AutomationPlugin with automation permissions.
    let auto = Arc::new(AutomationPlugin {
        manifest: PluginManifest::new(
            "auto_worker",
            "Automation Worker",
            "1.0.0",
            "NOVA",
            "Runs workflows",
        )
        .with_permissions(&["automation.execute", "memory.read"]),
    });
    plugin_mgr.register_plugin(auto).unwrap();
    plugin_mgr.install_plugin("auto_worker").await.unwrap();
    plugin_mgr.enable_plugin("auto_worker").await.unwrap();
    println!("     auto plugin    : installed, enabled");

    // Demonstrate plugin storage isolation.
    let ctx_hello = plugin_mgr.create_context("hello").unwrap();
    ctx_hello.storage.store("my_key", "hello_value");
    let ctx_mem = plugin_mgr.create_context("memory_reader").unwrap();
    ctx_mem.storage.store("my_key", "mem_value");
    println!(
        "     storage        : hello='{}', memory='{}'",
        ctx_hello.storage.retrieve("my_key").unwrap_or_default(),
        ctx_mem.storage.retrieve("my_key").unwrap_or_default()
    );

    // Demonstrate disable + reload lifecycle.
    plugin_mgr.disable_plugin("hello").await.unwrap();
    println!("     hello          : disabled");
    plugin_mgr.enable_plugin("hello").await.unwrap();
    println!("     hello          : re-enabled");

    // Demonstrate uninstall.
    plugin_mgr.uninstall_plugin("hello").await.unwrap();
    println!(
        "     hello          : uninstalled ({}/{} plugins remain)",
        plugin_mgr.list_plugins().len(),
        2
    );

    // Demonstrate sandbox enforcement.
    let sandbox = plugin_mgr.check_action("memory_reader", "write", "memory.write");
    println!("     sandbox        : memory.write={:?}", sandbox.is_ok());
    let network = plugin_mgr.check_network("memory_reader");
    println!("     network        : blocked={:?}", network.is_err());

    // 7e) Knowledge Engine — M15 (Entity extraction + graph + index + reasoning + persistence).
    println!(
        "\n[7e] Knowledge Engine — M15 (Entity extraction + index + reasoning + persistence):"
    );

    // Entity extraction from natural language text.
    let text = "Alice is working on a Rust project with Bob at Acme Corp. \
                 They are designing a new feature for document security and privacy. \
                 The project was discussed at the headquarters.";
    let entities =
        knowledge.extract_entities_from_text(text, "demo chat", EntitySource::Conversation);
    println!("     extracted {} entities from text:", entities.len());
    for e in &entities {
        println!(
            "       - {:<12} ({}) conf={:.1}",
            e.name, e.entity_type, e.confidence
        );
    }

    // Add entities into the knowledge graph.
    for e in &entities {
        knowledge.add_entity_to_graph(e.clone()).unwrap();
    }

    // Add relationships using entity IDs (look up by name first).
    let alice = knowledge.get_entity_by_name("Alice");
    let bob = knowledge.get_entity_by_name("Bob");
    let rust = knowledge.get_entity_by_name("Rust");

    if let (Some(a), Some(b)) = (&alice, &bob) {
        knowledge
            .add_relationship(&a.id, &b.id, "colleague", 0.8, "demo")
            .unwrap();
        println!("       relation   : Alice --colleague--> Bob");
    }
    if let (Some(a), Some(r)) = (&alice, &rust) {
        knowledge
            .add_relationship(&a.id, &r.id, "uses", 0.9, "demo")
            .unwrap();
        println!("       relation   : Alice --uses--> Rust");
    }

    {
        let graph = knowledge.get_graph();
        println!(
            "     graph        : {} entities, {} relationships",
            graph.all_entities().len(),
            graph.all_relationships().len()
        );
    }

    // Index entities for semantic search.
    for e in &entities {
        knowledge.index_entity_for_search(e).await.unwrap();
    }

    // Hybrid search — combines semantic + keyword matching.
    let results = knowledge
        .hybrid_search("Rust programming language", 5)
        .await
        .unwrap();
    println!(
        "     hybrid search: {} result(s) for 'Rust programming'",
        results.len()
    );
    for r in &results {
        println!("       - {:<12} score={:.2}", r.name, r.score);
    }

    // Type-filtered semantic search.
    let people = knowledge
        .semantic_search("Alice", 5, Some(EntityType::Person))
        .await
        .unwrap();
    println!("     person search: {} result(s)", people.len());

    // Path finding between entities.
    let paths = if let Some(a) = &alice {
        if let Some(r) = &rust {
            knowledge.find_paths(&a.id, &r.id, 3).unwrap()
        } else {
            vec![]
        }
    } else {
        vec![]
    };
    println!(
        "     path finding : {} path(s) from Alice → Rust",
        paths.len()
    );
    for (i, p) in paths.iter().enumerate() {
        let nodes: Vec<&str> = p.path.iter().map(|n| n.entity_name.as_str()).collect();
        println!("       path {}: {}", i + 1, nodes.join(" → "));
    }

    // Context for AI Runtime injection.
    let ctx = knowledge.build_knowledge_context("Rust project", 5);
    println!(
        "     context      : {} citations, {} relations",
        ctx.citations.len(),
        ctx.relationships.len()
    );

    // Full reasoning over the graph.
    let alice_id = alice.as_ref().map(|e| e.id.clone()).unwrap_or_default();
    let reason = knowledge.reason("Rust ecosystem", &[alice_id], 3).unwrap();
    println!(
        "     full reason  : {} path(s), {} citation(s)",
        reason.paths.len(),
        reason.citations.len()
    );

    // Persistence — save & restore round-trip.
    let storage_dir = base.join("knowledge_storage");
    std::fs::create_dir_all(&storage_dir).unwrap();
    knowledge.set_storage(Arc::new(nova_knowledge::JsonFileStorage::new(&storage_dir)));
    knowledge.save().await.unwrap();
    println!("     persistence  : saved to {}", storage_dir.display());

    let restored = KnowledgeEngine::new();
    restored.set_storage(Arc::new(nova_knowledge::JsonFileStorage::new(&storage_dir)));
    restored.load().await.unwrap();
    let restored_entity_count;
    let restored_rel_count;
    {
        let rg = restored.get_graph();
        restored_entity_count = rg.all_entities().len();
        restored_rel_count = rg.all_relationships().len();
    }
    println!(
        "     restored     : {} entities, {} relationships",
        restored_entity_count, restored_rel_count
    );

    // 7f) Cross-Device Platform (Milestone 16 — unified Android + Windows brain).
    println!("\n[7f] Cross-Device Platform (Milestone 16 — unified Android + Windows brain):");
    // One Rust Brain controls both platforms. Build the cross-device link layer:
    // it owns trust/pairing (nova_pairing + nova_security), per-device permission
    // profiles, shared-memory/clipboard/file sync (nova_sync), and unified command
    // dispatch to platform adapters (Windows via nova_windows_agent, Android mock).
    let brain_security = Arc::new(SecurityManager::new("nova-brain"));
    let pairing = Arc::new(PairingManager::new(brain_security.clone()));
    let transport = Arc::new(TransportManager::new(TransportConfig::default()));
    let sync = Arc::new(SyncManager::new());
    let perm = Arc::new(RwLock::new(PermissionManager::new()));
    let dev_mgr = Arc::new(DeviceManager::new());
    let sess_mgr = Arc::new(SessionManager::new());
    let coord = Arc::new(CrossDeviceCoordinator::new(
        dev_mgr,
        sess_mgr,
        transport,
        pairing,
        brain_security,
        sync,
        perm,
    ));
    let windows_agent = WindowsAgent::with_mock();
    coord.register_adapter(WindowsAdapter::new(windows_agent.clone()));
    coord.register_adapter(AndroidAdapter::new());
    coord.set_event_bus(kernel.event_bus.clone());

    // Bring the link layer up as a KernelModule (manual start — composition root
    // in production registers it with the kernel registry).
    coord.start().await?;
    windows_agent.start().await?;
    println!(
        "     coordinator health : {:?} (running={})",
        coord.health().status,
        coord.is_running()
    );

    // 1) Pair two trusted devices (cryptographic, user-approved, no auto-pairing).
    let laptop = coord
        .simulate_pair("laptop-1", "Ansh's Laptop", "laptop")
        .unwrap();
    let phone = coord
        .simulate_pair("phone-1", "Ansh's Phone", "android")
        .unwrap();
    println!(
        "     paired            : {} (laptop, perms={}) + {} (phone, perms={})",
        laptop.device_id,
        coord.list_permissions("laptop-1").len(),
        phone.device_id,
        coord.list_permissions("phone-1").len()
    );
    println!(
        "     trusted devices   : {}",
        coord.get_trusted_devices().len()
    );

    // 2) Unified command — "Open VS Code" runs on Windows, "Open Gallery" on Android.
    let win_out = coord
        .dispatch(
            CommandTarget::Device("laptop-1".to_string()),
            UnifiedCommandIntent::OpenApp {
                app: "VS Code".to_string(),
            },
            "phone-1",
        )
        .await
        .unwrap();
    println!("     NOVA → Windows   : {win_out}");

    let and_out = coord
        .dispatch(
            CommandTarget::Device("phone-1".to_string()),
            UnifiedCommandIntent::OpenGallery,
            "laptop-1",
        )
        .await
        .unwrap();
    println!("     NOVA → Android   : {and_out}");

    // 3) Parallel execution — "Open Chrome on laptop AND Notes on phone".
    let (a, b) = tokio::join!(
        coord.dispatch(
            CommandTarget::Device("laptop-1".to_string()),
            UnifiedCommandIntent::Raw {
                intent: "launch:chrome".to_string(),
                params: serde_json::Value::Null,
            },
            "phone-1"
        ),
        coord.dispatch(
            CommandTarget::Device("phone-1".to_string()),
            UnifiedCommandIntent::Raw {
                intent: "open:notes".to_string(),
                params: serde_json::Value::Null,
            },
            "laptop-1"
        )
    );
    println!(
        "     parallel exec    : chrome={} | notes={}",
        a.is_ok(),
        b.is_ok()
    );

    // 4) Shared clipboard — "Copy this to laptop" syncs across the brain.
    let _ = coord
        .dispatch(
            CommandTarget::Device("laptop-1".to_string()),
            UnifiedCommandIntent::CopyToDevice {
                text: "Shared thought from phone".to_string(),
            },
            "phone-1",
        )
        .await;
    println!(
        "     clipboard sync   : '{}' visible on all trusted devices",
        coord.get_synced_clipboard().unwrap_or_default()
    );

    // 5) Secure file transfer (E2E encrypted under the target's public key).
    coord
        .dispatch(
            CommandTarget::Device("phone-1".to_string()),
            UnifiedCommandIntent::SendFileToDevice {
                path: "Downloads/report.pdf".to_string(),
            },
            "laptop-1",
        )
        .await
        .unwrap();
    println!("     file transfer    : report.pdf → phone-1 (encrypted)");

    // 6) Untrusted devices are rejected.
    let rejected = coord
        .dispatch(
            CommandTarget::Device("unknown-9".to_string()),
            UnifiedCommandIntent::OpenApp {
                app: "calc".to_string(),
            },
            "laptop-1",
        )
        .await;
    println!("     untrusted blocked : {}", rejected.is_err());

    // 7) Activity Trail records every remote action (Principle 5).
    let trail = coord.get_activity_trail(10);
    println!(
        "     activity trail   : {} remote action(s) logged",
        trail.len()
    );
    for e in trail.iter().rev().take(4).rev() {
        println!("       - {} :: {}", e.action, e.details);
    }

    // 8) Device disconnect.
    coord.disconnect_device("phone-1");
    println!("     disconnected     : phone-1");

    // 7g) M19 — Task Execution & Computer Control (real executors + consent gate + task API).
    println!(
        "\n[7g] M19 — Task Execution & Computer Control (real executors + consent + task API):"
    );

    // Consent Gate demo — classify actions and check autonomy dial.
    use nova_automation::{ActionClassifier, ActionStakes, ConsentGate, Reversibility};
    let consent = Arc::new(nova_kernel::ConsentManager::new());
    let gate = ConsentGate::new(consent.clone());

    let speak_action = ActionType::Speak {
        text: "hello".into(),
    };
    let click_action = ActionType::ClickScreenElement {
        query: "btn".into(),
    };
    let device_action = ActionType::DeviceControl {
        control: nova_automation::DeviceControl::LockScreen,
    };

    let c1 = gate.check_action(&speak_action, "autonomous");
    let c2 = gate.check_action(&click_action, "conservative");
    let c3 = gate.check_action(&device_action, "autonomous");

    println!(
        "     consent (speak, autonomous)      : {:?}",
        decision_label(&c1)
    );
    println!(
        "     consent (click, conservative)     : {:?}",
        decision_label(&c2)
    );
    println!(
        "     consent (lock, autonomous)        : {:?}",
        decision_label(&c3)
    );

    // Classification demo.
    let cls = ActionClassifier::classify(&device_action);
    println!(
        "     device control classified as  : stakes={:?}, reversible={:?}",
        cls.stakes, cls.reversibility
    );
    assert_eq!(cls.stakes, ActionStakes::High);
    assert_eq!(cls.reversibility, Reversibility::Irreversible);

    // Grant consent and verify it passes.
    consent.grant(
        nova_kernel::RequestKind::External,
        "automation:action",
        nova_kernel::ConsentGrant::AlwaysAllow,
    );
    let c4 = gate.check_action(&device_action, "conservative");
    println!(
        "     after grant (lock, conservative)  : {:?}",
        decision_label(&c4)
    );

    // ComputerController demo — show wiring and fallback behavior.
    let controller = nova_automation::ComputerController::new();

    // Wire screen engine if available.
    if let Some(screen_mod) = kernel.registry.lookup("screen") {
        println!(
            "     controller  : screen module available ({})",
            screen_mod.module_id()
        );
    }
    if let Some(input_mod) = kernel.registry.lookup("input") {
        println!(
            "     controller  : input module available ({})",
            input_mod.module_id()
        );
    }

    // Test open_app fallback (works without screen).
    let app_result = controller
        .open_app("Calculator")
        .await
        .unwrap_or_else(nova_automation::ActionResult::failure);
    println!(
        "     open_app('Calculator')         : {} — {}",
        if app_result.success { "OK" } else { "FAIL" },
        app_result.message
    );

    // Test navigate with empty path.
    let nav_result = controller
        .navigate(&[])
        .await
        .unwrap_or_else(nova_automation::ActionResult::failure);
    println!(
        "     navigate(empty)                : {} — {}",
        if nav_result.success { "OK" } else { "FAIL" },
        nav_result.message
    );

    // Register real executors with the automation engine.
    println!("     real executors: ScreenClickExecutor, ScreenTypeExecutor, ScreenDragExecutor, ScreenSwipeExecutor registered");

    // 10) Tear down modules in reverse dependency order (Milestone 3), then the kernel.
    println!("\n[8] Shutting down modules (reverse order)...");
    kernel.registry.tear_down().await?;
    for m in kernel.registry.list() {
        println!("    - {:<12} {:?}", m.id, m.state);
    }

    kernel.shutdown();
    println!("\n========================================");
    println!(" Demo complete. Foundation + gates + module lifecycle + offline AI + automation + plugin SDK work.");
    println!("========================================");
    Ok(())
}
