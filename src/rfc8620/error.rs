//! JMAP error objects (RFC 8620): the request-level error answering a
//! request refused whole (§3.6.1), the method-level error returned in a
//! failed method response (§3.6.2) and the per-object `Foo/set` error
//! (§5.3).

use core::{error::Error, fmt};

#[cfg(feature = "client")]
use alloc::string::ToString;
use alloc::{string::String, vec::Vec};

use serde::{Deserialize, Serialize};

/// The namespace of the JMAP problem types (RFC 8620 §3.6.1).
#[cfg(feature = "client")]
const JMAP_ERROR_URN: &str = "urn:ietf:params:jmap:error:";

/// A JMAP request-level error (RFC 8620 §3.6.1): the problem details
/// (RFC 7807) of a request the server refused whole.
///
/// NOTE: variants keep the struct shape even with a single field, the
/// internally-tagged serde representation of the wire object requires
/// it.
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type")]
pub enum JmapRequestError {
    /// The request names a capability the server does not support.
    #[serde(rename = "urn:ietf:params:jmap:error:unknownCapability")]
    UnknownCapability {
        /// Optional human-readable detail.
        detail: Option<String>,
    },
    /// The request body was not valid JSON.
    #[serde(rename = "urn:ietf:params:jmap:error:notJSON")]
    NotJson {
        /// Optional human-readable detail.
        detail: Option<String>,
    },
    /// The request parsed as JSON but is not a valid Request object.
    #[serde(rename = "urn:ietf:params:jmap:error:notRequest")]
    NotRequest {
        /// Optional human-readable detail.
        detail: Option<String>,
    },
    /// The request exceeds a limit of the server.
    #[serde(rename = "urn:ietf:params:jmap:error:limit")]
    Limit {
        /// The name of the exceeded limit, as the core capability spells
        /// it (`maxCallsInRequest`, `maxSizeRequest`…).
        limit: String,
        /// Optional human-readable detail.
        detail: Option<String>,
    },
    /// A problem type of the JMAP namespace this library does not model,
    /// kept as the server named it.
    #[serde(untagged)]
    Other {
        /// The problem type, a URN.
        #[serde(rename = "type")]
        type_name: String,
        /// Optional human-readable detail.
        detail: Option<String>,
    },
}

impl JmapRequestError {
    /// Reads the body of a refused request, `None` unless it holds a
    /// problem of the JMAP namespace.
    #[cfg(feature = "client")]
    pub(crate) fn from_body(body: &[u8]) -> Option<Self> {
        match serde_json::from_slice(body).ok()? {
            Self::Other { type_name, .. } if !type_name.starts_with(JMAP_ERROR_URN) => None,
            err => Some(err),
        }
    }
}

impl fmt::Display for JmapRequestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownCapability { detail } => write_described(f, "unknownCapability", detail),
            Self::NotJson { detail } => write_described(f, "notJSON", detail),
            Self::NotRequest { detail } => write_described(f, "notRequest", detail),
            Self::Limit { limit, detail } => {
                write!(f, "JMAP limit ({limit})")?;
                if let Some(detail) = detail {
                    write!(f, ": {detail}")?;
                }
                Ok(())
            }
            Self::Other { type_name, detail } => write_described(f, type_name, detail),
        }
    }
}

impl Error for JmapRequestError {}

/// A JMAP method-level error (RFC 8620 §3.6.2).
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
    /// One of the method arguments is invalid.
    InvalidArguments {
        /// Optional human-readable detail.
        description: Option<String>,
        /// The invalid argument names, which some servers (Fastmail) send.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        arguments: Vec<String>,
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
    /// The `ifInState` of a `Foo/set` no longer matches the server's state
    /// (RFC 8620 §5.3): nothing was changed.
    StateMismatch {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// The `anchor` of a `Foo/query` is not in the results (RFC 8620
    /// §5.5).
    AnchorNotFound {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// A `Foo/query` sort is unsupported by the server (RFC 8620 §5.5),
    /// e.g. an `Email/query` property missing from the mail capability's
    /// `emailQuerySortOptions` (RFC 8621 §1.3.1).
    UnsupportedSort {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// A `Foo/query` filter is unsupported by the server (RFC 8620
    /// §5.5).
    UnsupportedFilter {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// A method error type this library does not model, kept as the server
    /// named it.
    #[serde(untagged)]
    Other {
        /// The error type, empty when the response named none.
        #[serde(rename = "type")]
        type_name: String,
        /// Optional human-readable detail.
        description: Option<String>,
    },
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
            Self::InvalidArguments {
                description,
                arguments,
            } => {
                write!(f, "JMAP invalidArguments")?;
                if !arguments.is_empty() {
                    write!(f, " [{}]", arguments.join(", "))?;
                }
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
            Self::StateMismatch { description } => write_described(f, "stateMismatch", description),
            Self::AnchorNotFound { description } => {
                write_described(f, "anchorNotFound", description)
            }
            Self::UnsupportedSort { description } => {
                write_described(f, "unsupportedSort", description)
            }
            Self::UnsupportedFilter { description } => {
                write_described(f, "unsupportedFilter", description)
            }
            Self::Other {
                type_name,
                description,
            } => write_described(f, type_name, description),
        }
    }
}

impl JmapMethodError {
    /// The error a method response named `error` reads as when its
    /// arguments are no error object, the parse failure as description.
    #[cfg(feature = "client")]
    pub(crate) fn unreadable(err: serde_json::Error) -> Self {
        Self::Other {
            type_name: String::new(),
            description: Some(err.to_string()),
        }
    }
}

impl Error for JmapMethodError {}

/// Writes one wire error type the way [`JmapMethodError`] displays its
/// own, followed by its description when the server gave one; the
/// per-object `Foo/set` errors display through it.
pub(crate) fn write_described(
    f: &mut fmt::Formatter<'_>,
    kind: &str,
    description: &Option<String>,
) -> fmt::Result {
    write!(f, "JMAP {kind}")?;
    if let Some(description) = description {
        write!(f, ": {description}")?;
    }
    Ok(())
}

/// Writes an `invalidProperties` error the way [`JmapMethodError`]
/// displays its own: the properties named, then the description.
pub(crate) fn write_properties(
    f: &mut fmt::Formatter<'_>,
    description: &Option<String>,
    properties: &[String],
) -> fmt::Result {
    write!(f, "JMAP invalidProperties")?;
    if !properties.is_empty() {
        write!(f, " [{}]", properties.join(", "))?;
    }
    if let Some(description) = description {
        write!(f, ": {description}")?;
    }
    Ok(())
}

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

#[cfg(test)]
mod tests {
    use alloc::string::ToString;

    use crate::rfc8620::error::JmapMethodError;

    #[test]
    fn an_unmodelled_type_keeps_what_the_server_said() {
        let err: JmapMethodError = serde_json::from_value(serde_json::json!({
            "type": "fromAccountNotFound",
            "description": "Not a valid accountId",
        }))
        .unwrap();

        assert_eq!(
            err.to_string(),
            "JMAP fromAccountNotFound: Not a valid accountId"
        );
    }

    #[test]
    fn invalid_arguments_name_the_arguments() {
        let err: JmapMethodError = serde_json::from_value(serde_json::json!({
            "type": "invalidArguments",
            "arguments": ["filter/inMailbox"],
        }))
        .unwrap();

        assert_eq!(err.to_string(), "JMAP invalidArguments [filter/inMailbox]");
    }
}
