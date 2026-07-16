//! JMAP error objects (RFC 8620): the method-level error returned in a
//! failed method response (§3.6.1) and the per-object `Foo/set` error
//! (§5.3).

use core::{error::Error, fmt};

use alloc::{string::String, vec::Vec};

use serde::{Deserialize, Serialize};

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
