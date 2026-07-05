use crate::error::{ErrorCategory, NovaError, Result};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::sync::OnceLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivacyConfig {
    pub local_by_default: bool,
    pub allow_remote_acceleration: bool,
    pub telemetry_enabled: bool,
    pub allowed_egress_domains: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    pub db_path: String,
    pub max_memory_entries: usize,
    pub auto_pruning: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationConfig {
    pub autonomy_level: String, // "conservative", "moderate", "autonomous"
    pub require_consent_for_destructive: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemConfig {
    pub device_tier: String, // "low", "medium", "high"
    pub log_level: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NovaConfig {
    pub privacy: PrivacyConfig,
    pub memory: MemoryConfig,
    pub automation: AutomationConfig,
    pub system: SystemConfig,
}

impl Default for NovaConfig {
    fn default() -> Self {
        Self {
            privacy: PrivacyConfig {
                local_by_default: true,
                allow_remote_acceleration: false, // D3 default: private
                telemetry_enabled: false,         // default off
                allowed_egress_domains: vec![],
            },
            memory: MemoryConfig {
                db_path: "local-store/nova.db".to_string(),
                max_memory_entries: 100000,
                auto_pruning: false,
            },
            automation: AutomationConfig {
                autonomy_level: "conservative".to_string(), // D8 default: conservative
                require_consent_for_destructive: true,
            },
            system: SystemConfig {
                device_tier: "medium".to_string(),
                log_level: "info".to_string(),
            },
        }
    }
}

impl NovaConfig {
    pub fn validate(&self) -> Result<()> {
        let level = self.automation.autonomy_level.as_str();
        if level != "conservative" && level != "moderate" && level != "autonomous" {
            return Err(NovaError::new(
                ErrorCategory::ConfigInvalid,
                "ERR_CONFIG_001",
                &format!("Invalid autonomy level: {}. Must be 'conservative', 'moderate', or 'autonomous'", level)
            ));
        }

        let tier = self.system.device_tier.as_str();
        if tier != "low" && tier != "medium" && tier != "high" {
            return Err(NovaError::new(
                ErrorCategory::ConfigInvalid,
                "ERR_CONFIG_002",
                &format!(
                    "Invalid device tier: {}. Must be 'low', 'medium', or 'high'",
                    tier
                ),
            ));
        }

        Ok(())
    }
}

static CURRENT_CONFIG: OnceLock<RwLock<NovaConfig>> = OnceLock::new();

fn get_config_lock() -> &'static RwLock<NovaConfig> {
    CURRENT_CONFIG.get_or_init(|| RwLock::new(NovaConfig::default()))
}

/// Retrieve a copy of the active system configuration
pub fn get_config() -> NovaConfig {
    get_config_lock().read().clone()
}

/// Update configuration manually and validate it
pub fn update_config(new_config: NovaConfig) -> Result<()> {
    new_config.validate()?;
    *get_config_lock().write() = new_config;
    Ok(())
}

/// Loads config file from standard path, merging layered overrides
pub fn load_config_from_dir(config_dir: &Path) -> Result<NovaConfig> {
    let mut config = NovaConfig::default();

    // 1. Load default config
    let default_path = config_dir.join("default.toml");
    if default_path.exists() {
        let content = fs::read_to_string(&default_path).map_err(|e| {
            NovaError::new(
                ErrorCategory::ConfigInvalid,
                "ERR_CONFIG_LOAD_FAIL",
                &format!("Failed to read default.toml: {}", e),
            )
        })?;
        let parsed: NovaConfig = toml::from_str(&content).map_err(|e| {
            NovaError::new(
                ErrorCategory::ConfigInvalid,
                "ERR_CONFIG_PARSE_FAIL",
                &format!("Failed to parse default.toml: {}", e),
            )
        })?;
        config = parsed;
    }

    // 2. Load local user overrides
    let local_path = config_dir.join("local.toml");
    if local_path.exists() {
        let content = fs::read_to_string(&local_path).map_err(|e| {
            NovaError::new(
                ErrorCategory::ConfigInvalid,
                "ERR_CONFIG_LOAD_FAIL",
                &format!("Failed to read local.toml: {}", e),
            )
        })?;
        // For simplicity in the skeleton, we parse a full overrides config.
        // In production, we'd use partial deserialization or merging.
        let parsed: NovaConfig = toml::from_str(&content).map_err(|e| {
            NovaError::new(
                ErrorCategory::ConfigInvalid,
                "ERR_CONFIG_PARSE_FAIL",
                &format!("Failed to parse local.toml: {}", e),
            )
        })?;
        config = parsed;
    }

    config.validate()?;
    *get_config_lock().write() = config.clone();

    Ok(config)
}
