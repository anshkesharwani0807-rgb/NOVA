use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationConfig {
    pub scheduler_tick_ms: u64,
    pub default_workflow_timeout_ms: u64,
    pub max_retries: u32,
    pub retry_delay_ms: u64,
    pub max_concurrent_workflows: usize,
    pub history_max_entries: usize,
    pub enable_scheduler: bool,
    pub enable_voice_triggers: bool,
    pub enable_vision_triggers: bool,
    pub enable_memory_triggers: bool,
    pub enable_device_triggers: bool,
}

impl Default for AutomationConfig {
    fn default() -> Self {
        Self {
            scheduler_tick_ms: 10_000,
            default_workflow_timeout_ms: 30_000,
            max_retries: 3,
            retry_delay_ms: 1_000,
            max_concurrent_workflows: 10,
            history_max_entries: 500,
            enable_scheduler: true,
            enable_voice_triggers: false,
            enable_vision_triggers: false,
            enable_memory_triggers: false,
            enable_device_triggers: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let cfg = AutomationConfig::default();
        assert!(cfg.scheduler_tick_ms >= 100);
        assert!(cfg.enable_scheduler);
    }

    #[test]
    fn test_config_clone() {
        let cfg = AutomationConfig::default();
        let cloned = cfg.clone();
        assert_eq!(cfg.scheduler_tick_ms, cloned.scheduler_tick_ms);
    }
}
