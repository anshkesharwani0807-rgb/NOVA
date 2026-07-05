//! NOVA developer demo — a runnable end-to-end smoke test of the kernel.
//!
//! This is NOT the product UI (there is no app/screen yet). It exists so a developer
//! can *see* the kernel work: it boots, starts a few module skeletons, drives one
//! pub/sub event and one request/response through the Event Bus, and prints the
//! privacy-first config defaults plus the user-facing activity and egress trails.
//!
//! Run it with:  cargo run -p nova_demo

use std::sync::Arc;

use nova_ai::AIEngine;
use nova_kernel::{
    get_config, get_recent_activity, get_recent_egress, ConsentGrant, EgressPolicy, EgressRequest,
    EventMetadata, Kernel, NovaEvent, RequestKind,
};
use nova_memory::MemoryEngine;
use nova_search::UniversalSearch;

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

    // 4) Start a few module skeletons (they subscribe / register handlers).
    println!("\n[2] Starting modules (memory, search, ai)...");
    let memory = MemoryEngine::new(kernel.clone());
    let search = UniversalSearch::new(kernel.clone());
    let ai = AIEngine::new(kernel.clone());
    memory.start().await?;
    search.start().await?;
    ai.start().await?;

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
    let query: Arc<String> = Arc::new("birthday photos 2019".to_string());
    let response = kernel
        .event_bus
        .request("search:query", smeta, query)
        .await?;
    let body = response
        .payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .unwrap_or("<non-text response>");
    println!("    search response: {body}");

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

    kernel.shutdown();
    println!("\n========================================");
    println!(" Demo complete. Foundation + Consent/Egress gates work. Features come next.");
    println!("========================================");
    Ok(())
}
