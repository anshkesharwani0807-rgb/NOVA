use std::sync::Arc;

use nova_kernel::{ConsentManager, ConsentResolution, RequestKind};

use crate::action::ActionType;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionStakes {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reversibility {
    Reversible,
    Irreversible,
}

#[derive(Debug, Clone)]
pub struct ActionClassification {
    pub stakes: ActionStakes,
    pub reversibility: Reversibility,
    pub description: String,
}

pub struct ActionClassifier;

impl ActionClassifier {
    pub fn classify(action: &ActionType) -> ActionClassification {
        match action {
            ActionType::Speak { .. } => ActionClassification {
                stakes: ActionStakes::Low,
                reversibility: Reversibility::Reversible,
                description: "speak text".into(),
            },
            ActionType::Notify { .. } => ActionClassification {
                stakes: ActionStakes::Low,
                reversibility: Reversibility::Reversible,
                description: "show notification".into(),
            },
            ActionType::OpenApp { .. } => ActionClassification {
                stakes: ActionStakes::Medium,
                reversibility: Reversibility::Reversible,
                description: "open application".into(),
            },
            ActionType::LaunchActivity { .. } => ActionClassification {
                stakes: ActionStakes::Medium,
                reversibility: Reversibility::Reversible,
                description: "launch activity".into(),
            },
            ActionType::Clipboard { action: _, text: _ } => ActionClassification {
                stakes: ActionStakes::Medium,
                reversibility: Reversibility::Irreversible,
                description: "clipboard operation".into(),
            },
            ActionType::CreateMemory { .. } => ActionClassification {
                stakes: ActionStakes::Low,
                reversibility: Reversibility::Reversible,
                description: "create memory".into(),
            },
            ActionType::SearchMemory { .. } => ActionClassification {
                stakes: ActionStakes::Low,
                reversibility: Reversibility::Reversible,
                description: "search memory".into(),
            },
            ActionType::RunAI { .. } => ActionClassification {
                stakes: ActionStakes::Medium,
                reversibility: Reversibility::Irreversible,
                description: "run AI inference".into(),
            },
            ActionType::CaptureVoice { .. } => ActionClassification {
                stakes: ActionStakes::Medium,
                reversibility: Reversibility::Irreversible,
                description: "capture voice audio".into(),
            },
            ActionType::AnalyzeImage { .. } => ActionClassification {
                stakes: ActionStakes::Medium,
                reversibility: Reversibility::Reversible,
                description: "analyze image".into(),
            },
            ActionType::DeviceControl { control: _ } => ActionClassification {
                stakes: ActionStakes::High,
                reversibility: Reversibility::Irreversible,
                description: "device control".into(),
            },
            ActionType::PluginInvocation { .. } => ActionClassification {
                stakes: ActionStakes::High,
                reversibility: Reversibility::Irreversible,
                description: "plugin invocation".into(),
            },
            ActionType::Wait { .. } => ActionClassification {
                stakes: ActionStakes::Low,
                reversibility: Reversibility::Reversible,
                description: "wait".into(),
            },
            ActionType::SubWorkflow { .. } => ActionClassification {
                stakes: ActionStakes::Medium,
                reversibility: Reversibility::Reversible,
                description: "execute sub-workflow".into(),
            },
            ActionType::InputInjection(_) => ActionClassification {
                stakes: ActionStakes::High,
                reversibility: Reversibility::Irreversible,
                description: "inject input".into(),
            },
            ActionType::ClickScreenElement { .. } => ActionClassification {
                stakes: ActionStakes::High,
                reversibility: Reversibility::Irreversible,
                description: "click screen element".into(),
            },
            ActionType::TypeIntoScreenElement { .. } => ActionClassification {
                stakes: ActionStakes::High,
                reversibility: Reversibility::Irreversible,
                description: "type into screen element".into(),
            },
            ActionType::ClickScreenText { .. } => ActionClassification {
                stakes: ActionStakes::High,
                reversibility: Reversibility::Irreversible,
                description: "click text on screen".into(),
            },
            ActionType::DragScreenElements { .. } => ActionClassification {
                stakes: ActionStakes::High,
                reversibility: Reversibility::Irreversible,
                description: "drag screen elements".into(),
            },
            ActionType::SwipeScreenElements { .. } => ActionClassification {
                stakes: ActionStakes::High,
                reversibility: Reversibility::Irreversible,
                description: "swipe screen elements".into(),
            },
        }
    }
}

#[derive(Debug, Clone)]
pub enum ConsentDecision {
    Allowed,
    Blocked { reason: String },
    RequiresPrompt { stakes: ActionStakes, description: String },
}

pub struct ConsentGate {
    consent_manager: Arc<ConsentManager>,
}

impl ConsentGate {
    pub fn new(consent_manager: Arc<ConsentManager>) -> Self {
        Self { consent_manager }
    }

    pub fn check_action(
        &self,
        action: &ActionType,
        autonomy_level: &str,
    ) -> ConsentDecision {
        let classification = ActionClassifier::classify(action);
        let request_kind = action_to_request_kind(action);

        let resolution = self.consent_manager.authorize(request_kind, "automation:action");

        match resolution {
            ConsentResolution::Granted(_) => ConsentDecision::Allowed,
            ConsentResolution::Denied => ConsentDecision::Blocked {
                reason: "user has denied this type of action".into(),
            },
            ConsentResolution::RequiresPrompt => {
                match autonomy_level {
                    "autonomous" => {
                        match classification.stakes {
                            ActionStakes::Low => ConsentDecision::Allowed,
                            ActionStakes::Medium => {
                                if classification.reversibility == Reversibility::Reversible {
                                    ConsentDecision::Allowed
                                } else {
                                    ConsentDecision::RequiresPrompt {
                                        stakes: classification.stakes,
                                        description: classification.description,
                                    }
                                }
                            }
                            ActionStakes::High => ConsentDecision::RequiresPrompt {
                                stakes: classification.stakes,
                                description: classification.description,
                            },
                        }
                    }
                    "moderate" => {
                        match classification.stakes {
                            ActionStakes::Low => {
                                if classification.reversibility == Reversibility::Reversible {
                                    ConsentDecision::Allowed
                                } else {
                                    ConsentDecision::RequiresPrompt {
                                        stakes: classification.stakes,
                                        description: classification.description,
                                    }
                                }
                            }
                            ActionStakes::Medium | ActionStakes::High => {
                                ConsentDecision::RequiresPrompt {
                                    stakes: classification.stakes,
                                    description: classification.description,
                                }
                            }
                        }
                    }
                    _ => {
                        ConsentDecision::RequiresPrompt {
                            stakes: classification.stakes,
                            description: classification.description,
                        }
                    }
                }
            }
        }
    }

    pub fn consent_manager(&self) -> &ConsentManager {
        &self.consent_manager
    }
}

fn action_to_request_kind(_action: &ActionType) -> RequestKind {
    RequestKind::External
}

#[cfg(test)]
mod tests {
    use super::*;
    use nova_kernel::ConsentGrant;

    fn make_gate() -> ConsentGate {
        ConsentGate::new(Arc::new(ConsentManager::new()))
    }

    #[test]
    fn test_conservative_blocks_everything_by_default() {
        let gate = make_gate();
        let action = ActionType::Speak { text: "hi".into() };
        let decision = gate.check_action(&action, "conservative");
        match decision {
            ConsentDecision::RequiresPrompt { .. } => {}
            other => panic!("expected RequiresPrompt, got {other:?}"),
        }
    }

    #[test]
    fn test_autonomous_allows_low_stakes() {
        let gate = make_gate();
        let action = ActionType::Speak { text: "hi".into() };
        let decision = gate.check_action(&action, "autonomous");
        assert!(matches!(decision, ConsentDecision::Allowed));
    }

    #[test]
    fn test_autonomous_blocks_high_stakes() {
        let gate = make_gate();
        let action = ActionType::DeviceControl {
            control: crate::action::DeviceControl::LockScreen,
        };
        let decision = gate.check_action(&action, "autonomous");
        match decision {
            ConsentDecision::RequiresPrompt { stakes: ActionStakes::High, .. } => {}
            other => panic!("expected RequiresPrompt(High), got {other:?}"),
        }
    }

    #[test]
    fn test_moderate_allows_low_reversible() {
        let gate = make_gate();
        let action = ActionType::Wait { duration_ms: 100 };
        let decision = gate.check_action(&action, "moderate");
        assert!(matches!(decision, ConsentDecision::Allowed));
    }

    #[test]
    fn test_granted_action_is_allowed() {
        let gate = make_gate();
        gate.consent_manager.grant(
            RequestKind::External,
            "automation:action",
            ConsentGrant::AlwaysAllow,
        );
        let action = ActionType::InputInjection(crate::action::InputInjectionParams {
            action_type: "click".into(),
            params: std::collections::HashMap::new(),
        });
        let decision = gate.check_action(&action, "conservative");
        assert!(matches!(decision, ConsentDecision::Allowed));
    }

    #[test]
    fn test_classifier_low_stakes() {
        let c = ActionClassifier::classify(&ActionType::Speak { text: "hello".into() });
        assert_eq!(c.stakes, ActionStakes::Low);
        assert_eq!(c.reversibility, Reversibility::Reversible);
    }

    #[test]
    fn test_classifier_high_stakes() {
        let c = ActionClassifier::classify(&ActionType::DeviceControl {
            control: crate::action::DeviceControl::LockScreen,
        });
        assert_eq!(c.stakes, ActionStakes::High);
        assert_eq!(c.reversibility, Reversibility::Irreversible);
    }

    #[test]
    fn test_classifier_screen_actions_high() {
        let c = ActionClassifier::classify(&ActionType::ClickScreenElement { query: "btn".into() });
        assert_eq!(c.stakes, ActionStakes::High);
        assert_eq!(c.reversibility, Reversibility::Irreversible);
    }
}
