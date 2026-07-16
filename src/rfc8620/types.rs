//! JMAP Core (RFC 8620) data types: session, method errors,
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
    /// The username associated with the credentials used to fetch the
    /// session.
    pub username: String,
    /// The accounts the user has access to, keyed by account id.
    pub accounts: BTreeMap<String, JmapAccountInfo>,
    /// The primary account id per capability URN.
    pub primary_accounts: BTreeMap<String, String>,
    /// The capabilities the server supports, keyed by capability URN.
    pub capabilities: BTreeMap<String, Value>,
    /// The URL to POST JMAP API requests to.
    pub api_url: Url,
    /// The blob download URL template (RFC 6570).
    pub download_url: String,
    /// The blob upload URL template (RFC 6570).
    pub upload_url: String,
    /// The URL of the event source push channel.
    pub event_source_url: String,
    /// The opaque server state; changes when the session object changes.
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
    /// The human-readable account name.
    pub name: String,
    /// Whether the account belongs to the authenticated user.
    pub is_personal: bool,
    /// Whether the account is read-only.
    pub is_read_only: bool,
    /// Account-level capability objects, keyed by capability URN.
    pub account_capabilities: BTreeMap<String, Value>,
}

/// A JMAP method-level error (RFC 8620 §3.6.1).
///
/// NOTE: variants keep the struct shape even with a single field, the
/// internally-tagged serde representation of the wire object requires
/// it.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum JmapMethodError {
    /// An unexpected or unknown error occurred during the method call.
    ServerFail {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// Some, but not all, expected changes were applied.
    ServerPartialFail,
    /// The server is currently unable to run the method.
    ServerUnavailable {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// The method requires a capability the request did not declare.
    UnknownCapability {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// The request body was not valid JSON.
    NotJson {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// The request parsed as JSON but is not a valid Request object.
    NotRequest {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// A server-defined limit was exceeded.
    Limit {
        /// Optional human-readable detail.
        description: Option<String>,
        /// The name of the exceeded limit.
        limit: String,
    },
    /// One of the method arguments is invalid.
    InvalidArguments {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// Access denied for this method call (RFC 8620 §3.6.2), e.g. requesting
    /// the `url` or `keys` properties in `PushSubscription/get` (§7.2.1).
    Forbidden {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// The total request size exceeds the server limit.
    RequestTooLarge,
    /// The referenced object does not exist.
    NotFound,
    /// A `Foo/set` update patch is invalid.
    InvalidPatch {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// The object will be destroyed by this request, so it cannot be
    /// updated.
    WillDestroy {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// One or more object properties are invalid.
    InvalidProperties {
        /// Optional human-readable detail.
        description: Option<String>,
        /// The invalid property names.
        #[serde(default)]
        properties: Vec<String>,
    },
    /// The type is a singleton, objects cannot be created or destroyed.
    Singleton,
    /// The method name is not known by the server.
    UnknownMethod {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// Server can no longer compute changes from `sinceState` (RFC 8620 §5.2):
    /// callers MUST fall back to `Foo/get` and resume from the returned state.
    CannotCalculateChanges {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// Any error type this library does not know about.
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
    /// The wire error type (`invalidProperties`, `forbidden`, …).
    pub r#type: String,
    /// Optional human-readable detail.
    pub description: Option<String>,
    /// The properties the error relates to, when applicable.
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

/// A JMAP result reference (RFC 8620 §3.7) used to back-reference an
/// earlier method call's result within a batch request.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapResultReference<'a> {
    /// The call id of the method call to reference.
    pub result_of: &'a str,
    /// The name of the referenced method.
    pub name: &'static str,
    /// The JSON pointer into the referenced result.
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
    /// The id of the added object.
    pub id: String,
    /// The zero-based position of the object in the query results.
    pub index: u64,
}

#[cfg(test)]
mod tests {
    use alloc::{string::String, vec};

    use serde::{Deserialize, Serialize};
    use serde_json::json;

    use crate::rfc8620::*;

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
