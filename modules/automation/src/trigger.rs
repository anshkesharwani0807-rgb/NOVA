use chrono::{Datelike, Local, Timelike};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TriggerType {
    Time {
        hour: u32,
        minute: u32,
        days_of_week: Option<Vec<u32>>,
    },
    Date {
        year: Option<i32>,
        month: Option<u32>,
        day: Option<u32>,
    },
    Battery {
        level: u32,
        above: bool,
    },
    Charging {
        state: ChargingState,
    },
    WiFi {
        ssid: Option<String>,
        connected: bool,
    },
    Bluetooth {
        device_name: Option<String>,
        connected: bool,
    },
    DeviceState {
        state: DeviceState,
    },
    Memory {
        category: Option<String>,
        keyword: Option<String>,
        event: String,
    },
    Voice {
        phrase: String,
    },
    Vision {
        event: String,
    },
    Manual,
    EventBus {
        event_name: String,
        filter: Option<HashMap<String, String>>,
    },
    Plugin {
        plugin_id: String,
        event: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ChargingState {
    Charging,
    Discharging,
    Full,
    NotCharging,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DeviceState {
    ScreenOn,
    ScreenOff,
    Locked,
    Unlocked,
    Idle,
    Active,
    DoNotDisturb,
    PowerSave,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerResult {
    pub triggered: bool,
    pub reason: String,
    pub context: HashMap<String, String>,
}

impl TriggerResult {
    pub fn not_triggered() -> Self {
        Self {
            triggered: false,
            reason: String::new(),
            context: HashMap::new(),
        }
    }

    pub fn triggered(reason: impl Into<String>) -> Self {
        let mut ctx = HashMap::new();
        ctx.insert("trigger_time".to_string(), Local::now().to_rfc3339());
        Self {
            triggered: true,
            reason: reason.into(),
            context: ctx,
        }
    }
}

pub trait TriggerEvaluator: Send + Sync {
    fn evaluate(&self, trigger: &TriggerType, context: &HashMap<String, String>) -> TriggerResult;
    fn kind(&self) -> &'static str;
}

pub struct TimeTriggerEvaluator;

impl TriggerEvaluator for TimeTriggerEvaluator {
    fn evaluate(&self, trigger: &TriggerType, _context: &HashMap<String, String>) -> TriggerResult {
        match trigger {
            TriggerType::Time {
                hour,
                minute,
                days_of_week,
            } => {
                let now = Local::now();
                if now.hour() == *hour && now.minute() == *minute {
                    if let Some(days) = days_of_week {
                        let today = now.weekday().num_days_from_monday();
                        if !days.contains(&today) {
                            return TriggerResult::not_triggered();
                        }
                    }
                    TriggerResult::triggered(format!("time trigger at {:02}:{:02}", hour, minute))
                } else {
                    TriggerResult::not_triggered()
                }
            }
            TriggerType::Date { year, month, day } => {
                let now = Local::now();
                let year_match = year.is_none_or(|y| now.year() == y);
                let month_match = month.is_none_or(|m| now.month() == m);
                let day_match = day.is_none_or(|d| now.day() == d);
                if year_match && month_match && day_match {
                    TriggerResult::triggered("date trigger matched")
                } else {
                    TriggerResult::not_triggered()
                }
            }
            _ => TriggerResult::not_triggered(),
        }
    }

    fn kind(&self) -> &'static str {
        "time"
    }
}

pub struct ManualTriggerEvaluator;

impl TriggerEvaluator for ManualTriggerEvaluator {
    fn evaluate(&self, trigger: &TriggerType, _context: &HashMap<String, String>) -> TriggerResult {
        if matches!(trigger, TriggerType::Manual) {
            TriggerResult::triggered("manual trigger")
        } else {
            TriggerResult::not_triggered()
        }
    }

    fn kind(&self) -> &'static str {
        "manual"
    }
}

pub struct EventBusTriggerEvaluator;

impl TriggerEvaluator for EventBusTriggerEvaluator {
    fn evaluate(&self, trigger: &TriggerType, context: &HashMap<String, String>) -> TriggerResult {
        match trigger {
            TriggerType::EventBus { event_name, filter } => {
                let actual = context.get("event_name").map(|s| s.as_str()).unwrap_or("");
                if actual != event_name.as_str() {
                    return TriggerResult::not_triggered();
                }
                if let Some(f) = filter {
                    for (k, v) in f {
                        if context.get(k).map(|s| s.as_str()) != Some(v.as_str()) {
                            return TriggerResult::not_triggered();
                        }
                    }
                }
                TriggerResult::triggered(format!("event bus trigger: {}", event_name))
            }
            _ => TriggerResult::not_triggered(),
        }
    }

    fn kind(&self) -> &'static str {
        "event_bus"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_manual_trigger_evaluator() {
        let eval = ManualTriggerEvaluator;
        let result = eval.evaluate(&TriggerType::Manual, &HashMap::new());
        assert!(result.triggered);
    }

    #[test]
    fn test_manual_not_matched_for_other() {
        let eval = ManualTriggerEvaluator;
        let result = eval.evaluate(
            &TriggerType::Time {
                hour: 0,
                minute: 0,
                days_of_week: None,
            },
            &HashMap::new(),
        );
        assert!(!result.triggered);
    }

    #[test]
    fn test_event_bus_trigger_evaluator_matched() {
        let eval = EventBusTriggerEvaluator;
        let mut ctx = HashMap::new();
        ctx.insert("event_name".into(), "test.event".into());
        let result = eval.evaluate(
            &TriggerType::EventBus {
                event_name: "test.event".into(),
                filter: None,
            },
            &ctx,
        );
        assert!(result.triggered);
    }

    #[test]
    fn test_event_bus_trigger_evaluator_not_matched() {
        let eval = EventBusTriggerEvaluator;
        let mut ctx = HashMap::new();
        ctx.insert("event_name".into(), "other.event".into());
        let result = eval.evaluate(
            &TriggerType::EventBus {
                event_name: "test.event".into(),
                filter: None,
            },
            &ctx,
        );
        assert!(!result.triggered);
    }

    #[test]
    fn test_time_trigger_evaluator_kind() {
        let eval = TimeTriggerEvaluator;
        assert_eq!(eval.kind(), "time");
    }

    #[test]
    fn test_trigger_result_defaults() {
        let r = TriggerResult::not_triggered();
        assert!(!r.triggered);
        assert!(r.reason.is_empty());
    }

    #[test]
    fn test_trigger_result_triggered() {
        let r = TriggerResult::triggered("test reason");
        assert!(r.triggered);
        assert_eq!(r.reason, "test reason");
    }
}
