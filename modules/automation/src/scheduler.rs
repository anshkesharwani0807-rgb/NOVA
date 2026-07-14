use chrono::{Datelike, Local, Timelike};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

use crate::config::AutomationConfig;
use crate::trigger::{TimeTriggerEvaluator, TriggerEvaluator, TriggerResult, TriggerType};
use crate::workflow::Workflow;

pub struct Scheduler {
    config: AutomationConfig,
    trigger_evaluators: HashMap<String, Box<dyn TriggerEvaluator>>,
    last_tick: RwLock<i64>,
}

impl Scheduler {
    pub fn new(config: AutomationConfig) -> Self {
        let mut evaluators: HashMap<String, Box<dyn TriggerEvaluator>> = HashMap::new();
        evaluators.insert("time".to_string(), Box::new(TimeTriggerEvaluator));
        Self {
            config,
            trigger_evaluators: evaluators,
            last_tick: RwLock::new(0),
        }
    }

    pub fn register_evaluator(&mut self, evaluator: Box<dyn TriggerEvaluator>) {
        self.trigger_evaluators
            .insert(evaluator.kind().to_string(), evaluator);
    }

    pub fn check_triggers(
        &self,
        workflows: &[Arc<Workflow>],
        context: &HashMap<String, String>,
    ) -> Vec<(Arc<Workflow>, TriggerResult)> {
        let now = chrono::Local::now().timestamp_millis();
        let mut triggered = Vec::new();

        for wf in workflows {
            if !wf.enabled {
                continue;
            }
            for tc in &wf.triggers {
                let kind = trigger_kind(&tc.trigger);
                if let Some(evaluator) = self.trigger_evaluators.get(kind) {
                    let result = evaluator.evaluate(&tc.trigger, context);
                    if result.triggered {
                        triggered.push((wf.clone(), result));
                        break;
                    }
                }
            }
        }

        *self.last_tick.write() = now;
        triggered
    }

    pub fn tick_interval_ms(&self) -> u64 {
        self.config.scheduler_tick_ms
    }

    pub fn get_next_scheduled(&self, workflows: &[Arc<Workflow>]) -> Option<(Arc<Workflow>, i64)> {
        let now = Local::now();
        let mut next: Option<(Arc<Workflow>, i64)> = None;

        for wf in workflows {
            if !wf.enabled {
                continue;
            }
            for tc in &wf.triggers {
                if let TriggerType::Time {
                    hour,
                    minute,
                    days_of_week,
                } = &tc.trigger
                {
                    let mut target = now
                        .with_hour(*hour)
                        .and_then(|d| d.with_minute(*minute))
                        .and_then(|d| d.with_second(0))
                        .map(|d| d.timestamp_millis());

                    if let Some(t) = target {
                        if t <= now.timestamp_millis() {
                            target = Some(t + 86400000);
                        }
                    }

                    if let Some(ref mut t) = target {
                        if let Some(days) = days_of_week {
                            let dt =
                                chrono::DateTime::from_timestamp_millis(*t).unwrap_or_default();
                            let target_day = dt.weekday().num_days_from_monday();
                            if !days.contains(&target_day) {
                                continue;
                            }
                        }
                        let is_earlier = next.as_ref().is_none_or(|(_, n)| *t < *n);
                        if is_earlier {
                            next = Some((wf.clone(), *t));
                        }
                    }
                }
            }
        }

        next
    }
}

fn trigger_kind(trigger: &TriggerType) -> &str {
    match trigger {
        TriggerType::Time { .. } | TriggerType::Date { .. } => "time",
        TriggerType::Manual => "manual",
        TriggerType::EventBus { .. } => "event_bus",
        _ => "time",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AutomationConfig;
    use crate::workflow::Workflow;
    use std::collections::HashMap;
    use std::sync::Arc;

    fn make_wf(id: &str, trigger: crate::trigger::TriggerType) -> Workflow {
        let mut wf = Workflow::new(id.into(), id.into(), "desc".into());
        wf.triggers.push(crate::workflow::TriggerConfig {
            trigger,
            conditions: None,
        });
        wf
    }

    #[test]
    fn test_tick_interval_default() {
        let cfg = AutomationConfig::default();
        let s = Scheduler::new(cfg);
        assert!(s.tick_interval_ms() >= 100);
    }

    #[test]
    fn test_check_triggers_time_matches_now() {
        let cfg = AutomationConfig::default();
        let s = Scheduler::new(cfg);
        let now = chrono::Local::now();
        let wf = make_wf(
            "s1",
            crate::trigger::TriggerType::Time {
                hour: now.hour(),
                minute: now.minute(),
                days_of_week: None,
            },
        );
        let workflows = vec![Arc::new(wf)];
        let ctx = HashMap::new();
        let triggered = s.check_triggers(&workflows, &ctx);
        assert_eq!(triggered.len(), 1);
        assert_eq!(triggered[0].0.id, "s1");
    }

    #[test]
    fn test_empty_workflows() {
        let cfg = AutomationConfig::default();
        let s = Scheduler::new(cfg);
        let ctx = HashMap::new();
        let triggered = s.check_triggers(&[], &ctx);
        assert!(triggered.is_empty());
    }

    #[test]
    fn test_disabled_workflows_skipped() {
        let cfg = AutomationConfig::default();
        let s = Scheduler::new(cfg);
        let now = chrono::Local::now();
        let mut wf = make_wf(
            "s2",
            crate::trigger::TriggerType::Time {
                hour: now.hour(),
                minute: now.minute(),
                days_of_week: None,
            },
        );
        wf.enabled = false;
        let workflows = vec![Arc::new(wf)];
        let ctx = HashMap::new();
        let triggered = s.check_triggers(&workflows, &ctx);
        assert!(triggered.is_empty());
    }

    #[test]
    fn test_get_next_scheduled_none() {
        let cfg = AutomationConfig::default();
        let s = Scheduler::new(cfg);
        let wf = make_wf("s3", crate::trigger::TriggerType::Manual);
        let workflows = vec![Arc::new(wf)];
        let next = s.get_next_scheduled(&workflows);
        assert!(next.is_none());
    }
}
