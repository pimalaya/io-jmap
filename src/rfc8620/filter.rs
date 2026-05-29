//! JMAP filter shape (RFC 8620 §5.5).
//!
//! The `filter` argument of a `Foo/query` request is either a
//! protocol-specific `FilterCondition` (e.g.
//! [`crate::rfc8621::email::EmailFilter`]) or a [`FilterOperator`]
//! combining sub-filters with AND / OR / NOT. The two forms are
//! discriminated on the wire by the presence of the `operator` key.

use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

/// A JMAP filter: either a protocol-specific condition (e.g.
/// [`crate::rfc8621::email::EmailFilter`]) or a logical combinator
/// over sub-filters.
///
/// `untagged` serialization picks the right wire shape automatically:
/// when an `operator` field is present the value is read as
/// [`FilterOperator`], otherwise it falls through to the condition.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Filter<C> {
    Operator(FilterOperator<C>),
    Condition(C),
}

impl<C> From<C> for Filter<C> {
    fn from(condition: C) -> Self {
        Filter::Condition(condition)
    }
}

impl<C> Filter<C> {
    /// Wraps `conditions` in an AND combinator.
    pub fn and(conditions: Vec<Filter<C>>) -> Self {
        Filter::Operator(FilterOperator {
            operator: FilterOperatorKind::And,
            conditions,
        })
    }

    /// Wraps `conditions` in an OR combinator.
    pub fn or(conditions: Vec<Filter<C>>) -> Self {
        Filter::Operator(FilterOperator {
            operator: FilterOperatorKind::Or,
            conditions,
        })
    }

    /// Wraps `conditions` in a NOT combinator. Per RFC 8620 §5.5 a
    /// `NOT` operator is satisfied when none of its conditions match,
    /// so a single-element vector models the unary logical negation.
    pub fn not(conditions: Vec<Filter<C>>) -> Self {
        Filter::Operator(FilterOperator {
            operator: FilterOperatorKind::Not,
            conditions,
        })
    }
}

/// Logical combinator over a list of sub-filters.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilterOperator<C> {
    pub operator: FilterOperatorKind,
    pub conditions: Vec<Filter<C>>,
}

/// AND / OR / NOT, serialized as RFC 8620 §5.5 spells them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum FilterOperatorKind {
    And,
    Or,
    Not,
}

#[cfg(test)]
mod tests {
    use alloc::{string::String, vec};

    use serde::{Deserialize, Serialize};
    use serde_json::json;

    use super::*;

    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
    struct Cond {
        from: Option<String>,
    }

    #[test]
    fn condition_serializes_flat() {
        let f: Filter<Cond> = Filter::Condition(Cond {
            from: Some("alice".into()),
        });
        assert_eq!(
            serde_json::to_value(&f).unwrap(),
            json!({ "from": "alice" })
        );
    }

    #[test]
    fn and_serializes_with_operator_key() {
        let f: Filter<Cond> = Filter::and(vec![
            Filter::Condition(Cond {
                from: Some("a".into()),
            }),
            Filter::Condition(Cond {
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
        let f: Filter<Cond> = Filter::not(vec![Filter::Condition(Cond {
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
        let f: Filter<Cond> = serde_json::from_value(v).unwrap();
        assert!(matches!(f, Filter::Condition(Cond { from: Some(_) })));

        let v = json!({
            "operator": "OR",
            "conditions": [{ "from": "a" }, { "from": "b" }],
        });
        let f: Filter<Cond> = serde_json::from_value(v).unwrap();
        assert!(matches!(
            f,
            Filter::Operator(FilterOperator {
                operator: FilterOperatorKind::Or,
                ..
            })
        ));
    }
}
