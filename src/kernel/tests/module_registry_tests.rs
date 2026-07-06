//! Integration tests for the Module Registry + lifecycle manager (Milestone 3).
//!
//! Exercise the public kernel API with a recording mock module to verify registration,
//! dependency resolution, lifecycle transitions, health reporting, duplicate-registration
//! protection, and shutdown order.

use async_trait::async_trait;
use nova_kernel::{HealthStatus, KernelModule, LifecycleState, ModuleHealth, ModuleRegistry};
use std::sync::{Arc, Mutex};

/// A module that records every lifecycle call into a shared log.
struct Recorder {
    id: &'static str,
    deps: Vec<&'static str>,
    log: Arc<Mutex<Vec<String>>>,
    health: HealthStatus,
}

#[async_trait]
impl KernelModule for Recorder {
    fn module_id(&self) -> &'static str {
        self.id
    }
    fn version(&self) -> &'static str {
        "0.1.0"
    }
    fn dependencies(&self) -> Vec<&'static str> {
        self.deps.clone()
    }
    async fn initialize(&self) -> nova_kernel::Result<()> {
        self.log.lock().unwrap().push(format!("init:{}", self.id));
        Ok(())
    }
    async fn start(&self) -> nova_kernel::Result<()> {
        self.log.lock().unwrap().push(format!("start:{}", self.id));
        Ok(())
    }
    async fn stop(&self) -> nova_kernel::Result<()> {
        self.log.lock().unwrap().push(format!("stop:{}", self.id));
        Ok(())
    }
    async fn shutdown(&self) -> nova_kernel::Result<()> {
        self.log
            .lock()
            .unwrap()
            .push(format!("shutdown:{}", self.id));
        Ok(())
    }
    fn health(&self) -> ModuleHealth {
        ModuleHealth {
            status: self.health,
            detail: String::new(),
        }
    }
}

fn recorder(
    id: &'static str,
    deps: &[&'static str],
    log: &Arc<Mutex<Vec<String>>>,
    health: HealthStatus,
) -> Arc<dyn KernelModule> {
    Arc::new(Recorder {
        id,
        deps: deps.to_vec(),
        log: log.clone(),
        health,
    })
}

#[test]
fn registration_lookup_and_duplicate_protection() {
    let reg = ModuleRegistry::new();
    let log = Arc::new(Mutex::new(Vec::new()));
    reg.register(recorder("memory", &[], &log, HealthStatus::Healthy))
        .unwrap();
    assert_eq!(reg.count(), 1);
    assert!(reg.contains("memory"));
    assert!(reg.lookup("memory").is_some());
    assert_eq!(reg.state("memory"), Some(LifecycleState::Boot));

    // Duplicate id is rejected.
    assert!(reg
        .register(recorder("memory", &[], &log, HealthStatus::Healthy))
        .is_err());

    // Unregister works; unregister-missing errors.
    reg.unregister("memory").unwrap();
    assert_eq!(reg.count(), 0);
    assert!(reg.unregister("memory").is_err());
}

#[tokio::test]
async fn lifecycle_transitions_and_shutdown_order() {
    let reg = ModuleRegistry::new();
    let log = Arc::new(Mutex::new(Vec::new()));

    // Linear dependency chain: b depends on a, c depends on b.
    reg.register(recorder("a", &[], &log, HealthStatus::Healthy))
        .unwrap();
    reg.register(recorder("b", &["a"], &log, HealthStatus::Healthy))
        .unwrap();
    reg.register(recorder("c", &["b"], &log, HealthStatus::Healthy))
        .unwrap();

    reg.bring_up().await.unwrap();
    // All running after bring-up.
    for id in ["a", "b", "c"] {
        assert_eq!(reg.state(id), Some(LifecycleState::Running), "{id} running");
    }

    reg.tear_down().await.unwrap();
    for id in ["a", "b", "c"] {
        assert_eq!(reg.state(id), Some(LifecycleState::Shutdown), "{id} down");
    }

    // Init + start in dependency order; stop + shutdown in reverse.
    let events = log.lock().unwrap().clone();
    assert_eq!(
        events,
        vec![
            "init:a",
            "init:b",
            "init:c",
            "start:a",
            "start:b",
            "start:c",
            "stop:c",
            "stop:b",
            "stop:a",
            "shutdown:c",
            "shutdown:b",
            "shutdown:a",
        ]
    );
}

#[tokio::test]
async fn health_report_reflects_module_health() {
    let reg = ModuleRegistry::new();
    let log = Arc::new(Mutex::new(Vec::new()));
    reg.register(recorder("ok", &[], &log, HealthStatus::Healthy))
        .unwrap();
    reg.register(recorder("sick", &[], &log, HealthStatus::Unhealthy))
        .unwrap();
    reg.bring_up().await.unwrap();

    let report = reg.health_report();
    let ok = report.iter().find(|(id, _)| id == "ok").unwrap();
    let sick = report.iter().find(|(id, _)| id == "sick").unwrap();
    assert_eq!(ok.1.status, HealthStatus::Healthy);
    assert_eq!(sick.1.status, HealthStatus::Unhealthy);
}

#[test]
fn missing_dependency_and_cycle_are_errors() {
    let log = Arc::new(Mutex::new(Vec::new()));

    let reg_missing = ModuleRegistry::new();
    reg_missing
        .register(recorder("a", &["ghost"], &log, HealthStatus::Healthy))
        .unwrap();
    assert!(reg_missing.resolve_order().is_err());

    let reg_cycle = ModuleRegistry::new();
    reg_cycle
        .register(recorder("a", &["b"], &log, HealthStatus::Healthy))
        .unwrap();
    reg_cycle
        .register(recorder("b", &["a"], &log, HealthStatus::Healthy))
        .unwrap();
    assert!(reg_cycle.resolve_order().is_err());
}
