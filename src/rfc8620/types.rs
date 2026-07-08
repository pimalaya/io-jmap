//! JMAP Core (RFC 8620) data types: capability URN, session, method errors,
//! request/response shape, filter combinator, result references.

use core::{error::Error, fmt};

use alloc::{collections::BTreeMap, format, string::String, vec::Vec};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::Url;

/// The JMAP session object returned by the well-known URL (RFC 8620 §2).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapSession {
    pub username: String,
    pub accounts: BTreeMap<String, JmapAccountInfo>,
    pub primary_accounts: BTreeMap<String, String>,
    pub capabilities: BTreeMap<String, Value>,
    pub api_url: Url,
    pub download_url: String,
    pub upload_url: String,
    pub event_source_url: String,
    pub state: String,
}

impl JmapSession {
    /// Returns the primary account ID for the given capability URN, or an empty
    /// string if none is advertised.
    pub fn primary_account_id_for(&self, capability: &str) -> String {
        self.primary_accounts
            .get(capability)
            .cloned()
            .unwrap_or_default()
    }
}

/// Information about a single JMAP account within a session.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapAccountInfo {
    pub name: String,
    pub is_personal: bool,
    pub is_read_only: bool,
    pub account_capabilities: BTreeMap<String, Value>,
}

/// A JMAP method-level error (RFC 8620 §3.6.1).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum JmapMethodError {
    ServerFail {
        description: Option<String>,
    },
    ServerPartialFail,
    ServerUnavailable {
        description: Option<String>,
    },
    UnknownCapability {
        description: Option<String>,
    },
    NotJson {
        description: Option<String>,
    },
    NotRequest {
        description: Option<String>,
    },
    Limit {
        description: Option<String>,
        limit: String,
    },
    InvalidArguments {
        description: Option<String>,
    },
    /// Access denied for this method call (RFC 8620 §3.6.2), e.g. requesting
    /// the `url` or `keys` properties in `PushSubscription/get` (§7.2.1).
    Forbidden {
        description: Option<String>,
    },
    RequestTooLarge,
    NotFound,
    InvalidPatch {
        description: Option<String>,
    },
    WillDestroy {
        description: Option<String>,
    },
    InvalidProperties {
        description: Option<String>,
        #[serde(default)]
        properties: Vec<String>,
    },
    Singleton,
    UnknownMethod {
        description: Option<String>,
    },
    /// Server can no longer compute changes from `sinceState` (RFC 8620 §5.2):
    /// callers MUST fall back to `Foo/get` and resume from the returned state.
    CannotCalculateChanges {
        description: Option<String>,
    },
    #[serde(other)]
    Unknown,
}

impl fmt::Display for JmapMethodError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ServerFail { description } => {
                write!(f, "JMAP serverFail")?;
                if let Some(d) = description {
                    write!(f, ": {d}")?;
                }
                Ok(())
            }
            Self::ServerPartialFail => write!(f, "JMAP serverPartialFail"),
            Self::ServerUnavailable { description } => {
                write!(f, "JMAP serverUnavailable")?;
                if let Some(d) = description {
                    write!(f, ": {d}")?;
                }
                Ok(())
            }
            Self::UnknownCapability { description } => {
                write!(f, "JMAP unknownCapability")?;
                if let Some(d) = description {
                    write!(f, ": {d}")?;
                }
                Ok(())
            }
            Self::NotJson { description } => {
                write!(f, "JMAP notJson")?;
                if let Some(d) = description {
                    write!(f, ": {d}")?;
                }
                Ok(())
            }
            Self::NotRequest { description } => {
                write!(f, "JMAP notRequest")?;
                if let Some(d) = description {
                    write!(f, ": {d}")?;
                }
                Ok(())
            }
            Self::Limit { description, limit } => {
                write!(f, "JMAP limit ({limit})")?;
                if let Some(d) = description {
                    write!(f, ": {d}")?;
                }
                Ok(())
            }
            Self::InvalidArguments { description } => {
                write!(f, "JMAP invalidArguments")?;
                if let Some(d) = description {
                    write!(f, ": {d}")?;
                }
                Ok(())
            }
            Self::Forbidden { description } => {
                write!(f, "JMAP forbidden")?;
                if let Some(d) = description {
                    write!(f, ": {d}")?;
                }
                Ok(())
            }
            Self::RequestTooLarge => write!(f, "JMAP requestTooLarge"),
            Self::NotFound => write!(f, "JMAP notFound"),
            Self::InvalidPatch { description } => {
                write!(f, "JMAP invalidPatch")?;
                if let Some(d) = description {
                    write!(f, ": {d}")?;
                }
                Ok(())
            }
            Self::WillDestroy { description } => {
                write!(f, "JMAP willDestroy")?;
                if let Some(d) = description {
                    write!(f, ": {d}")?;
                }
                Ok(())
            }
            Self::InvalidProperties {
                description,
                properties,
            } => {
                write!(f, "JMAP invalidProperties")?;
                if !properties.is_empty() {
                    write!(f, " [{}]", properties.join(", "))?;
                }
                if let Some(d) = description {
                    write!(f, ": {d}")?;
                }
                Ok(())
            }
            Self::Singleton => write!(f, "JMAP singleton"),
            Self::UnknownMethod { description } => {
                write!(f, "JMAP unknownMethod")?;
                if let Some(d) = description {
                    write!(f, ": {d}")?;
                }
                Ok(())
            }
            Self::CannotCalculateChanges { description } => {
                write!(f, "JMAP cannotCalculateChanges")?;
                if let Some(d) = description {
                    write!(f, ": {d}")?;
                }
                Ok(())
            }
            Self::Unknown => write!(f, "JMAP unknown error"),
        }
    }
}

impl Error for JmapMethodError {}

/// Per-object error returned in `Foo/set` responses (RFC 8620 §5.3).
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapSetError {
    pub r#type: String,
    pub description: Option<String>,
    #[serde(default)]
    pub properties: Vec<String>,
}

/// A JMAP filter (RFC 8620 §5.5): a protocol-specific condition (e.g.
/// [`crate::rfc8621::email::JmapEmailFilter`]) or a logical combinator.
///
/// `untagged` serde: presence of `operator` picks [`JmapFilterOperator`].
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JmapFilter<C> {
    Operator(JmapFilterOperator<C>),
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
    pub operator: JmapFilterOperatorKind,
    pub conditions: Vec<JmapFilter<C>>,
}

/// AND / OR / NOT, serialized as RFC 8620 §5.5 spells them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum JmapFilterOperatorKind {
    And,
    Or,
    Not,
}

/// A JMAP result reference (RFC 8620 §7.1) used to back-reference an
/// earlier method call's result within a batch request.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapResultReference<'a> {
    pub result_of: &'a str,
    pub name: &'static str,
    pub path: &'static str,
}

/// The JMAP Request object (RFC 8620 §3.3).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapRequest {
    /// Capability URNs required by the methods in this request.
    pub using: Vec<String>,

    /// The method calls to execute, as `(methodName, args, callId)`
    /// tuples.
    pub method_calls: Vec<(String, Value, String)>,

    /// Client-assigned IDs for newly created objects.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_ids: Option<BTreeMap<String, String>>,
}

/// The JMAP Response object (RFC 8620 §3.4).
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapResponse {
    /// Method responses in `(methodName, result, callId)` format.
    ///
    /// If a method failed, `methodName` is `"error"` and `result` is a
    /// [`JmapMethodError`] object.
    pub method_responses: Vec<(String, Value, String)>,

    /// Server-assigned IDs for objects created by this request.
    #[serde(default)]
    pub created_ids: Option<BTreeMap<String, String>>,

    /// The current state of the session after this request.
    pub session_state: String,
}

/// Builder for batched JMAP requests: multiple method calls in one HTTP
/// request, with generated call IDs for [`JmapResultReference`] back-refs.
#[derive(Debug, Default)]
pub struct JmapBatch {
    calls: Vec<(String, Value, String)>,
    counter: usize,
}

impl JmapBatch {
    /// Creates a new empty batch.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a method call. Returns the call ID (`"c0"`, `"c1"`, …) for use in
    /// back-references from later calls.
    pub fn add(&mut self, method: impl Into<String>, args: Value) -> String {
        let call_id = format!("c{}", self.counter);
        self.counter += 1;
        self.calls.push((method.into(), args, call_id.clone()));
        call_id
    }

    /// Consumes the batch and returns a [`JmapRequest`].
    pub fn into_request(self, using: Vec<String>) -> JmapRequest {
        JmapRequest {
            using,
            method_calls: self.calls,
            created_ids: None,
        }
    }
}

/// A single added item in a `Foo/queryChanges` response (RFC 8620 §5.6).
#[derive(Clone, Debug, Deserialize)]
pub struct JmapAddedItem {
    pub id: String,
    pub index: u64,
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use serde_json::json;

    use super::*;

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
