use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceConfig {
    pub clipboard_history_size: usize,
    pub sensor_polling_interval_ms: u64,
    pub battery_low_threshold_pct: u8,
    pub battery_critical_threshold_pct: u8,
    pub notification_filter_enabled: bool,
    pub notification_filter_patterns: Vec<String>,
    pub location_precision: String,
    pub monitor_clipboard: bool,
    pub monitor_battery: bool,
    pub monitor_connectivity: bool,
    pub monitor_storage: bool,
    pub monitor_sensors: bool,
    pub monitor_notifications: bool,
}

impl Default for DeviceConfig {
    fn default() -> Self {
        Self {
            clipboard_history_size: 50,
            sensor_polling_interval_ms: 5000,
            battery_low_threshold_pct: 20,
            battery_critical_threshold_pct: 5,
            notification_filter_enabled: false,
            notification_filter_patterns: vec![],
            location_precision: "coarse".to_string(),
            monitor_clipboard: true,
            monitor_battery: true,
            monitor_connectivity: true,
            monitor_storage: true,
            monitor_sensors: false,
            monitor_notifications: true,
        }
    }
}
