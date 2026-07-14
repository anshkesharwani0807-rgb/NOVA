use async_trait::async_trait;
use nova_kernel::EventBus;
use nova_plugin_sdk::{
    Plugin, PluginEventType, PluginLifecycleManager, PluginLoader, PluginManager, PluginManifest,
    PluginPermissionManager, PluginRegistry, PluginSandbox, PluginState, PluginStorage, Sandbox,
};
use std::sync::Arc;

struct TestPlugin {
    manifest: PluginManifest,
}

#[async_trait]
impl Plugin for TestPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }
}

fn make_test_plugin(id: &str, perms: &[&str]) -> Arc<dyn Plugin> {
    Arc::new(TestPlugin {
        manifest: PluginManifest::new(id, id, "1.0.0", "NOVA SDK Test", "Integration test plugin")
            .with_permissions(perms),
    })
}

#[tokio::test]
async fn test_integration_plugin_registration_and_lifecycle() {
    let event_bus = Arc::new(EventBus::new(64));
    let mgr = PluginManager::new(Some(event_bus));

    mgr.register_plugin(make_test_plugin("int_test", &["memory.read"]))
        .unwrap();
    assert_eq!(mgr.list_plugins().len(), 1);

    mgr.install_plugin("int_test").await.unwrap();
    assert_eq!(
        mgr.get_plugin("int_test").unwrap().state,
        PluginState::Installed
    );

    mgr.enable_plugin("int_test").await.unwrap();
    assert_eq!(
        mgr.get_plugin("int_test").unwrap().state,
        PluginState::Enabled
    );

    mgr.disable_plugin("int_test").await.unwrap();
    assert_eq!(
        mgr.get_plugin("int_test").unwrap().state,
        PluginState::Disabled
    );

    let health = mgr.check_health("int_test").unwrap();
    assert_eq!(health, "healthy");
}

#[tokio::test]
async fn test_integration_sandbox_enforcement() {
    let mgr = PluginManager::new(None);
    mgr.register_plugin(make_test_plugin(
        "sandbox_test",
        &["memory.read", "internet.access"],
    ))
    .unwrap();

    // Permissions are auto-declared by register_plugin; grant_all makes them active.
    mgr.permissions.grant_all("sandbox_test").unwrap();

    assert!(mgr
        .check_action("sandbox_test", "read", "memory.read")
        .is_ok());
    assert!(mgr
        .check_action("sandbox_test", "write", "memory.write")
        .is_err());
    assert!(mgr.check_network("sandbox_test").is_ok());

    mgr.permissions.revoke_all("sandbox_test");
    assert!(mgr.check_network("sandbox_test").is_err());
}

#[tokio::test]
async fn test_integration_plugin_context_and_storage() {
    let mgr = PluginManager::new(None);
    mgr.register_plugin(make_test_plugin("ctx_test", &[]))
        .unwrap();

    let ctx = mgr.create_context("ctx_test").unwrap();
    ctx.storage.store("key", "value");
    assert_eq!(ctx.storage.retrieve("key"), Some("value".to_string()));

    ctx.set_config("interval", "30");
    assert_eq!(ctx.get_config("interval"), Some("30".to_string()));

    ctx.permissions
        .declare("ctx_test", &["voice.listen".to_string()])
        .unwrap();
    ctx.permissions.grant_all("ctx_test").unwrap();
    assert!(ctx.check_permission("voice.listen").is_ok());
}

#[tokio::test]
async fn test_integration_full_lifecycle_with_event_bus() {
    let event_bus = Arc::new(EventBus::new(64));
    let mut rx = event_bus.subscribe();
    let mgr = PluginManager::new(Some(event_bus.clone()));

    mgr.register_plugin(make_test_plugin("event_test", &[]))
        .unwrap();
    mgr.install_plugin("event_test").await.unwrap();
    mgr.enable_plugin("event_test").await.unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let mut found_enable = false;
    while let Ok(event) = rx.try_recv() {
        if let Some(payload) = event
            .payload
            .downcast_ref::<nova_plugin_sdk::PluginEventPayload>()
        {
            if payload.event_type == PluginEventType::PluginEnabled {
                found_enable = true;
            }
        }
    }
    assert!(found_enable, "Should have received PluginEnabled event");
}

#[test]
fn test_integration_permission_edge_cases() {
    let pm = PluginPermissionManager::new();

    pm.declare("edge_plugin", &["a".to_string()]).unwrap();
    assert!(pm.declare("edge_plugin", &["b".to_string()]).is_err());

    pm.grant("edge_plugin", &["a".to_string()]);
    assert!(pm.check("edge_plugin", "a"));
    assert!(!pm.check("edge_plugin", "b"));

    pm.grant("edge_plugin", &["b".to_string()]);
    assert!(pm.check("edge_plugin", "b"));

    let granted = pm.granted_permissions("edge_plugin");
    assert_eq!(granted.len(), 2);

    pm.revoke_all("edge_plugin");
    assert_eq!(pm.granted_permissions("edge_plugin").len(), 0);
}

#[test]
fn test_integration_storage_isolation() {
    let s1 = PluginStorage::new_in_memory("plugin_a");
    let s2 = PluginStorage::new_in_memory("plugin_b");

    s1.store("shared_key", "value_a");
    s2.store("shared_key", "value_b");

    assert_eq!(s1.retrieve("shared_key"), Some("value_a".to_string()));
    assert_eq!(s2.retrieve("shared_key"), Some("value_b".to_string()));

    s1.clear();
    assert!(s1.retrieve("shared_key").is_none());
    assert_eq!(s2.retrieve("shared_key"), Some("value_b".to_string()));
}

#[test]
fn test_integration_loader_dependency_resolution() {
    let loader = PluginLoader::new();

    let mut child_manifest = PluginManifest::new("dep_child", "Child", "1.0", "NOVA", "child");
    child_manifest.dependencies.push("base".to_string());
    let base = make_test_plugin("base", &[]);
    let child = Arc::new(TestPlugin {
        manifest: child_manifest,
    });

    loader.register("base", base).unwrap();
    loader.register("dep_child", child).unwrap();

    assert!(loader.resolve_dependencies("dep_child").is_ok());
    assert!(loader.resolve_dependencies("base").is_ok());

    assert!(loader.resolve_dependencies("missing").is_err());
}

#[test]
fn test_integration_sandbox_isolated_from_other_plugins() {
    let pm = Arc::new(PluginPermissionManager::new());
    let sandbox = PluginSandbox::new(pm.clone());

    pm.declare("plugin_a", &["memory.read".to_string()])
        .unwrap();
    pm.declare("plugin_b", &["internet.access".to_string()])
        .unwrap();
    pm.grant_all("plugin_a").unwrap();
    pm.grant_all("plugin_b").unwrap();

    assert!(sandbox
        .validate_action("plugin_a", "read", "memory.read")
        .is_ok());
    assert!(sandbox
        .validate_action("plugin_a", "access", "internet.access")
        .is_err());
    assert!(sandbox
        .validate_action("plugin_b", "access", "internet.access")
        .is_ok());
    assert!(sandbox
        .validate_action("plugin_b", "read", "memory.read")
        .is_err());
}

#[test]
fn test_integration_registry_edge_cases() {
    let reg = PluginRegistry::new();

    assert_eq!(reg.count(), 0);
    assert!(!reg.contains("nobody"));

    reg.register("first", make_test_plugin("first", &[]))
        .unwrap();
    assert!(reg
        .register("first", make_test_plugin("first_dup", &[]))
        .is_err());

    reg.unregister("first").unwrap();
    assert!(reg.unregister("first").is_err());
}

#[tokio::test]
async fn test_integration_event_bus_events() {
    let event_bus = Arc::new(EventBus::new(64));
    let mut rx = event_bus.subscribe();
    let mgr = PluginManager::new(Some(event_bus.clone()));

    mgr.register_plugin(make_test_plugin("eventful", &[]))
        .unwrap();
    mgr.install_plugin("eventful").await.unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let mut found_load = false;
    while let Ok(event) = rx.try_recv() {
        if let Some(payload) = event
            .payload
            .downcast_ref::<nova_plugin_sdk::PluginEventPayload>()
        {
            if payload.event_type == PluginEventType::PluginLoaded {
                found_load = true;
                assert_eq!(payload.plugin_id, "eventful");
            }
        }
    }
    assert!(found_load);
}

#[tokio::test]
async fn test_integration_lifecycle_manager_direct() {
    let reg = Arc::new(PluginRegistry::new());
    let lm = PluginLifecycleManager::new(reg.clone());

    reg.register("direct", make_test_plugin("direct", &["memory.read"]))
        .unwrap();

    lm.install("direct").await.unwrap();
    assert_eq!(reg.state("direct"), Some(PluginState::Installed));

    lm.enable("direct", None).await.unwrap();
    assert_eq!(reg.state("direct"), Some(PluginState::Enabled));

    lm.disable("direct", None).await.unwrap();
    assert_eq!(reg.state("direct"), Some(PluginState::Disabled));

    lm.unload("direct", None).await.unwrap();
    assert_eq!(reg.state("direct"), Some(PluginState::Unloaded));

    lm.uninstall("direct", None).await.unwrap();
    assert!(!reg.contains("direct"));
}
