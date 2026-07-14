use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Condition {
    And(Vec<Condition>),
    Or(Vec<Condition>),
    Not(Box<Condition>),
    Comparison {
        field: String,
        operator: ComparisonOp,
        value: String,
    },
    Regex {
        field: String,
        pattern: String,
    },
    Contains {
        field: String,
        value: String,
    },
    Numeric {
        field: String,
        operator: NumericOp,
        value: f64,
    },
    DateCompare {
        field: String,
        operator: ComparisonOp,
        value: String,
    },
    PermissionCheck {
        permission: String,
    },
    ContextCheck {
        key: String,
        exists: bool,
    },
    True,
    False,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ComparisonOp {
    Eq,
    Neq,
    Gt,
    Gte,
    Lt,
    Lte,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NumericOp {
    Eq,
    Neq,
    Gt,
    Gte,
    Lt,
    Lte,
    Between { min: f64, max: f64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConditionResult {
    pub matched: bool,
    pub reason: String,
}

impl ConditionResult {
    pub fn matched(reason: impl Into<String>) -> Self {
        Self {
            matched: true,
            reason: reason.into(),
        }
    }

    pub fn not_matched(reason: impl Into<String>) -> Self {
        Self {
            matched: false,
            reason: reason.into(),
        }
    }
}

pub trait ConditionEvaluator: Send + Sync {
    fn evaluate(&self, condition: &Condition, context: &HashMap<String, String>)
        -> ConditionResult;
}

pub struct DefaultConditionEvaluator;

impl DefaultConditionEvaluator {
    fn get_field(&self, field: &str, context: &HashMap<String, String>) -> Option<String> {
        context.get(field).cloned()
    }
}

impl ConditionEvaluator for DefaultConditionEvaluator {
    fn evaluate(
        &self,
        condition: &Condition,
        context: &HashMap<String, String>,
    ) -> ConditionResult {
        match condition {
            Condition::And(conditions) => {
                for c in conditions {
                    let r = self.evaluate(c, context);
                    if !r.matched {
                        return ConditionResult::not_matched(r.reason);
                    }
                }
                ConditionResult::matched("all AND conditions matched")
            }
            Condition::Or(conditions) => {
                let mut reasons = Vec::new();
                for c in conditions {
                    let r = self.evaluate(c, context);
                    if r.matched {
                        return ConditionResult::matched(r.reason);
                    }
                    reasons.push(r.reason);
                }
                ConditionResult::not_matched(format!(
                    "no OR conditions matched: {}",
                    reasons.join("; ")
                ))
            }
            Condition::Not(inner) => {
                let r = self.evaluate(inner, context);
                if r.matched {
                    ConditionResult::not_matched(format!("NOT condition negated: {}", r.reason))
                } else {
                    ConditionResult::matched("NOT condition passed")
                }
            }
            Condition::Comparison {
                field,
                operator,
                value,
            } => {
                let actual = match self.get_field(field, context) {
                    Some(v) => v,
                    None => {
                        return ConditionResult::not_matched(format!("field '{}' not found", field))
                    }
                };
                let matched = match operator {
                    ComparisonOp::Eq => actual == *value,
                    ComparisonOp::Neq => actual != *value,
                    ComparisonOp::Gt => actual > *value,
                    ComparisonOp::Gte => actual >= *value,
                    ComparisonOp::Lt => actual < *value,
                    ComparisonOp::Lte => actual <= *value,
                };
                if matched {
                    ConditionResult::matched(format!("{} {} {}", field, format_op(operator), value))
                } else {
                    ConditionResult::not_matched(format!(
                        "{} is '{}', expected {} {}",
                        field,
                        actual,
                        format_op(operator),
                        value
                    ))
                }
            }
            Condition::Regex { field, pattern } => {
                let actual = match self.get_field(field, context) {
                    Some(v) => v,
                    None => {
                        return ConditionResult::not_matched(format!("field '{}' not found", field))
                    }
                };
                match Regex::new(pattern) {
                    Ok(re) => {
                        if re.is_match(&actual) {
                            ConditionResult::matched(format!("'{}' matches /{}/", actual, pattern))
                        } else {
                            ConditionResult::not_matched(format!(
                                "'{}' does not match /{}/",
                                actual, pattern
                            ))
                        }
                    }
                    Err(e) => {
                        ConditionResult::not_matched(format!("invalid regex '{}': {}", pattern, e))
                    }
                }
            }
            Condition::Contains { field, value } => {
                let actual = match self.get_field(field, context) {
                    Some(v) => v,
                    None => {
                        return ConditionResult::not_matched(format!("field '{}' not found", field))
                    }
                };
                if actual.contains(value.as_str()) {
                    ConditionResult::matched(format!("'{}' contains '{}'", field, value))
                } else {
                    ConditionResult::not_matched(format!(
                        "'{}' does not contain '{}'",
                        actual, value
                    ))
                }
            }
            Condition::Numeric {
                field,
                operator,
                value,
            } => {
                let actual_str = match self.get_field(field, context) {
                    Some(v) => v,
                    None => {
                        return ConditionResult::not_matched(format!("field '{}' not found", field))
                    }
                };
                let actual: f64 = match actual_str.parse() {
                    Ok(n) => n,
                    Err(_) => {
                        return ConditionResult::not_matched(format!(
                            "'{}' is not a number",
                            actual_str
                        ))
                    }
                };
                let matched = match operator {
                    NumericOp::Eq => (actual - value).abs() < f64::EPSILON,
                    NumericOp::Neq => (actual - value).abs() >= f64::EPSILON,
                    NumericOp::Gt => actual > *value,
                    NumericOp::Gte => actual >= *value,
                    NumericOp::Lt => actual < *value,
                    NumericOp::Lte => actual <= *value,
                    NumericOp::Between { min, max } => actual >= *min && actual <= *max,
                };
                if matched {
                    ConditionResult::matched(format!(
                        "{} {} {}",
                        field,
                        format_numeric_op(operator),
                        value
                    ))
                } else {
                    ConditionResult::not_matched(format!(
                        "{} is {}, expected {}",
                        field,
                        actual,
                        format_numeric_op(operator)
                    ))
                }
            }
            Condition::DateCompare {
                field,
                operator,
                value,
            } => {
                let actual = match self.get_field(field, context) {
                    Some(v) => v,
                    None => {
                        return ConditionResult::not_matched(format!("field '{}' not found", field))
                    }
                };
                let matched = match operator {
                    ComparisonOp::Eq => actual == *value,
                    ComparisonOp::Neq => actual != *value,
                    ComparisonOp::Gt => actual > *value,
                    ComparisonOp::Gte => actual >= *value,
                    ComparisonOp::Lt => actual < *value,
                    ComparisonOp::Lte => actual <= *value,
                };
                if matched {
                    ConditionResult::matched(format!(
                        "date {} {} {}",
                        field,
                        format_op(operator),
                        value
                    ))
                } else {
                    ConditionResult::not_matched(format!("date {} is '{}'", field, actual))
                }
            }
            Condition::PermissionCheck { permission } => {
                if context
                    .get("permissions")
                    .is_some_and(|p| p.contains(permission.as_str()))
                {
                    ConditionResult::matched(format!("permission '{}' granted", permission))
                } else {
                    ConditionResult::not_matched(format!("permission '{}' not granted", permission))
                }
            }
            Condition::ContextCheck { key, exists } => {
                let has = context.contains_key(key);
                if has == *exists {
                    ConditionResult::matched(format!(
                        "context key '{}' {}exists",
                        key,
                        if *exists { "" } else { "not " }
                    ))
                } else {
                    ConditionResult::not_matched(format!("context key '{}' exist={}", key, has))
                }
            }
            Condition::True => ConditionResult::matched("always true"),
            Condition::False => ConditionResult::not_matched("always false"),
        }
    }
}

fn format_op(op: &ComparisonOp) -> &'static str {
    match op {
        ComparisonOp::Eq => "==",
        ComparisonOp::Neq => "!=",
        ComparisonOp::Gt => ">",
        ComparisonOp::Gte => ">=",
        ComparisonOp::Lt => "<",
        ComparisonOp::Lte => "<=",
    }
}

fn format_numeric_op(op: &NumericOp) -> String {
    match op {
        NumericOp::Eq => "==".to_string(),
        NumericOp::Neq => "!=".to_string(),
        NumericOp::Gt => ">".to_string(),
        NumericOp::Gte => ">=".to_string(),
        NumericOp::Lt => "<".to_string(),
        NumericOp::Lte => "<=".to_string(),
        NumericOp::Between { min, max } => format!("between {} and {}", min, max),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn evaluate(c: &Condition, ctx: &HashMap<String, String>) -> ConditionResult {
        let evaluator = DefaultConditionEvaluator;
        evaluator.evaluate(c, ctx)
    }

    #[test]
    fn test_true() {
        let r = evaluate(&Condition::True, &HashMap::new());
        assert!(r.matched);
    }

    #[test]
    fn test_false() {
        let r = evaluate(&Condition::False, &HashMap::new());
        assert!(!r.matched);
    }

    #[test]
    fn test_comparison_eq() {
        let mut ctx = HashMap::new();
        ctx.insert("key".into(), "val".into());
        let r = evaluate(
            &Condition::Comparison {
                field: "key".into(),
                operator: ComparisonOp::Eq,
                value: "val".into(),
            },
            &ctx,
        );
        assert!(r.matched);
    }

    #[test]
    fn test_comparison_ne() {
        let mut ctx = HashMap::new();
        ctx.insert("key".into(), "val".into());
        let r = evaluate(
            &Condition::Comparison {
                field: "key".into(),
                operator: ComparisonOp::Neq,
                value: "other".into(),
            },
            &ctx,
        );
        assert!(r.matched);
    }

    #[test]
    fn test_contains() {
        let mut ctx = HashMap::new();
        ctx.insert("key".into(), "hello world".into());
        let r = evaluate(
            &Condition::Contains {
                field: "key".into(),
                value: "world".into(),
            },
            &ctx,
        );
        assert!(r.matched);
    }

    #[test]
    fn test_regex() {
        let mut ctx = HashMap::new();
        ctx.insert("key".into(), "abc123".into());
        let r = evaluate(
            &Condition::Regex {
                field: "key".into(),
                pattern: "^[a-z]+\\d+$".into(),
            },
            &ctx,
        );
        assert!(r.matched);
    }

    #[test]
    fn test_and_both_true() {
        let r = evaluate(
            &Condition::And(vec![Condition::True, Condition::True]),
            &HashMap::new(),
        );
        assert!(r.matched);
    }

    #[test]
    fn test_and_one_false() {
        let r = evaluate(
            &Condition::And(vec![Condition::True, Condition::False]),
            &HashMap::new(),
        );
        assert!(!r.matched);
    }

    #[test]
    fn test_or_both_false() {
        let r = evaluate(
            &Condition::Or(vec![Condition::False, Condition::False]),
            &HashMap::new(),
        );
        assert!(!r.matched);
    }

    #[test]
    fn test_or_one_true() {
        let r = evaluate(
            &Condition::Or(vec![Condition::False, Condition::True]),
            &HashMap::new(),
        );
        assert!(r.matched);
    }

    #[test]
    fn test_not() {
        let r = evaluate(&Condition::Not(Box::new(Condition::False)), &HashMap::new());
        assert!(r.matched);
    }

    #[test]
    fn test_numeric() {
        let mut ctx = HashMap::new();
        ctx.insert("score".into(), "42".into());
        let r = evaluate(
            &Condition::Numeric {
                field: "score".into(),
                operator: NumericOp::Gt,
                value: 10.0,
            },
            &ctx,
        );
        assert!(r.matched);
    }

    #[test]
    fn test_context_check() {
        let mut ctx = HashMap::new();
        ctx.insert("key".into(), "present".into());
        let r = evaluate(
            &Condition::ContextCheck {
                key: "key".into(),
                exists: true,
            },
            &ctx,
        );
        assert!(r.matched);
    }

    #[test]
    fn test_permission_check() {
        let mut ctx = HashMap::new();
        ctx.insert("permissions".into(), "test admin".into());
        let r = evaluate(
            &Condition::PermissionCheck {
                permission: "test".into(),
            },
            &ctx,
        );
        assert!(r.matched);
    }
}
