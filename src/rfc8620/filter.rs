//! JMAP filter combinator (RFC 8620 §5.5): a protocol-specific condition or
//! a logical AND / OR / NOT over sub-filters.

use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

/// A JMAP filter (RFC 8620 §5.5): a protocol-specific condition (e.g.
/// [`crate::rfc8621::email::query::JmapEmailFilter`]) or a logical
/// combinator.
///
/// `untagged` serde: presence of `operator` picks [`JmapFilterOperator`].
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JmapFilter<C> {
    /// A logical combinator over sub-filters.
    Operator(JmapFilterOperator<C>),
    /// A protocol-specific filter condition.
    Condition(C),
}

impl<C> From<C> for JmapFilter<C> {
    fn from(condition: C) -> Self {
        JmapFilter::Condition(condition)
    }
}

impl<C> JmapFilter<C> {
    /// Wraps `conditions` in an AND combinator.
    pub fn and(conditions: Vec<JmapFilter<C>>) -> Self {
        JmapFilter::Operator(JmapFilterOperator {
            operator: JmapFilterOperatorKind::And,
            conditions,
        })
    }

    /// Wraps `conditions` in an OR combinator.
    pub fn or(conditions: Vec<JmapFilter<C>>) -> Self {
        JmapFilter::Operator(JmapFilterOperator {
            operator: JmapFilterOperatorKind::Or,
            conditions,
        })
    }

    /// Wraps `conditions` in a NOT combinator. Satisfied when no condition
    /// matches (RFC 8620 §5.5), so a single-element vec models unary negation.
    pub fn not(conditions: Vec<JmapFilter<C>>) -> Self {
        JmapFilter::Operator(JmapFilterOperator {
            operator: JmapFilterOperatorKind::Not,
            conditions,
        })
    }
}

/// Logical combinator over a list of sub-filters.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapFilterOperator<C> {
    /// The logical operator combining the conditions.
    pub operator: JmapFilterOperatorKind,
    /// The sub-filters the operator applies to.
    pub conditions: Vec<JmapFilter<C>>,
}

/// AND / OR / NOT, serialized as RFC 8620 §5.5 spells them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum JmapFilterOperatorKind {
    /// All conditions must match.
    And,
    /// At least one condition must match.
    Or,
    /// No condition may match.
    Not,
}

#[cfg(test)]
mod tests {
    use alloc::{string::String, vec};

    use serde::{Deserialize, Serialize};
    use serde_json::json;

    use crate::rfc8620::filter::*;

    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
    struct Cond {
        from: Option<String>,
    }

    #[test]
    fn condition_serializes_flat() {
        let f: JmapFilter<Cond> = JmapFilter::Condition(Cond {
            from: Some("alice".into()),
        });
        assert_eq!(
            serde_json::to_value(&f).unwrap(),
            json!({ "from": "alice" })
        );
    }

    #[test]
    fn and_serializes_with_operator_key() {
        let f: JmapFilter<Cond> = JmapFilter::and(vec![
            JmapFilter::Condition(Cond {
                from: Some("a".into()),
            }),
            JmapFilter::Condition(Cond {
                from: Some("b".into()),
            }),
        ]);
        assert_eq!(
            serde_json::to_value(&f).unwrap(),
            json!({
                "operator": "AND",
                "conditions": [
                    { "from": "a" },
                    { "from": "b" },
                ],
            }),
        );
    }

    #[test]
    fn not_wraps_a_single_subfilter() {
        let f: JmapFilter<Cond> = JmapFilter::not(vec![JmapFilter::Condition(Cond {
            from: Some("a".into()),
        })]);
        assert_eq!(
            serde_json::to_value(&f).unwrap(),
            json!({
                "operator": "NOT",
                "conditions": [{ "from": "a" }],
            }),
        );
    }

    #[test]
    fn deserialize_discriminates_on_operator_key() {
        let v = json!({ "from": "alice" });
        let f: JmapFilter<Cond> = serde_json::from_value(v).unwrap();
        assert!(matches!(f, JmapFilter::Condition(Cond { from: Some(_) })));

        let v = json!({
            "operator": "OR",
            "conditions": [{ "from": "a" }, { "from": "b" }],
        });
        let f: JmapFilter<Cond> = serde_json::from_value(v).unwrap();
        assert!(matches!(
            f,
            JmapFilter::Operator(JmapFilterOperator {
                operator: JmapFilterOperatorKind::Or,
                ..
            })
        ));
    }
}
