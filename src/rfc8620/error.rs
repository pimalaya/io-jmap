//! JMAP error types (RFC 8620 §3.6).

use alloc::{string::String, vec::Vec};
use core::{error::Error, fmt};
use serde::{Deserialize, Serialize};

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
            Self::Unknown => write!(f, "JMAP unknown error"),
        }
    }
}

impl Error for JmapMethodError {}

/// Per-object error returned in `Foo/set` responses (RFC 8620 §5.3).
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetError {
    pub r#type: String,
    pub description: Option<String>,
    #[serde(default)]
    pub properties: Vec<String>,
}
