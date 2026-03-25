//! JMAP error types (RFC 8620 §3.6, RFC 8621).

use serde::{Deserialize, Serialize};

/// A JMAP method-level error (RFC 8620 §3.6).
///
/// When a JMAP method call fails, the server returns an error tuple
/// `["error", {"type": "...", ...}, "callId"]` in `method_responses`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum JmapMethodError {
    // ── RFC 8620 §3.6.1 errors ──────────────────────────────────────────

    /// The server encountered an unexpected error.
    ServerFail {
        description: Option<String>,
    },

    /// A partial response was sent but some methods failed.
    ServerPartialFail,

    /// The server is temporarily unavailable.
    ServerUnavailable {
        description: Option<String>,
    },

    /// The `using` array references a capability the server does not support.
    UnknownCapability {
        description: Option<String>,
    },

    /// The request body was not valid JSON.
    NotJson {
        description: Option<String>,
    },

    /// The request body was valid JSON but not a valid JMAP request.
    NotRequest {
        description: Option<String>,
    },

    /// A server-defined limit was exceeded (e.g. max batch size).
    Limit {
        description: Option<String>,
        limit: String,
    },

    // ── RFC 8621 method-level errors ─────────────────────────────────────

    /// The method arguments were invalid.
    InvalidArguments {
        description: Option<String>,
    },

    /// The method request was too large for the server to process.
    RequestTooLarge,

    /// The object was not found.
    NotFound,

    /// The patch is invalid.
    InvalidPatch {
        description: Option<String>,
    },

    /// The object cannot be updated because it would be destroyed.
    WillDestroy {
        description: Option<String>,
    },

    /// One or more properties were invalid.
    InvalidProperties {
        description: Option<String>,
        #[serde(default)]
        properties: Vec<String>,
    },

    /// The object is a singleton and cannot be created/destroyed.
    Singleton,

    /// The mailbox cannot be deleted because it has child mailboxes.
    MailboxHasChild,

    /// The mailbox cannot be deleted because it has email.
    MailboxHasEmail,

    /// A referenced blob was not found.
    BlobNotFound,

    /// The email would have too many keywords.
    TooManyKeywords,

    /// The email would be in too many mailboxes.
    TooManyMailboxes,

    /// The From address is not allowed.
    ForbiddenFrom,

    /// The email is invalid.
    InvalidEmail {
        description: Option<String>,
    },

    /// Too many recipients.
    TooManyRecipients,

    /// No valid recipients.
    NoRecipients,

    /// One or more recipients are invalid.
    InvalidRecipients {
        description: Option<String>,
    },

    /// The MAIL FROM address is not allowed.
    ForbiddenMailFrom,

    /// This user is not allowed to send email.
    ForbiddenToSend,

    /// The message has already been sent and cannot be unsent.
    CannotUnsendMessage,

    /// The method is not known to the server.
    UnknownMethod {
        description: Option<String>,
    },

    /// An unknown error type not yet enumerated.
    #[serde(other)]
    Unknown,
}

impl std::fmt::Display for JmapMethodError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
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
            Self::MailboxHasChild => write!(f, "JMAP mailboxHasChild"),
            Self::MailboxHasEmail => write!(f, "JMAP mailboxHasEmail"),
            Self::BlobNotFound => write!(f, "JMAP blobNotFound"),
            Self::TooManyKeywords => write!(f, "JMAP tooManyKeywords"),
            Self::TooManyMailboxes => write!(f, "JMAP tooManyMailboxes"),
            Self::ForbiddenFrom => write!(f, "JMAP forbiddenFrom"),
            Self::InvalidEmail { description } => {
                write!(f, "JMAP invalidEmail")?;
                if let Some(d) = description {
                    write!(f, ": {d}")?;
                }
                Ok(())
            }
            Self::TooManyRecipients => write!(f, "JMAP tooManyRecipients"),
            Self::NoRecipients => write!(f, "JMAP noRecipients"),
            Self::InvalidRecipients { description } => {
                write!(f, "JMAP invalidRecipients")?;
                if let Some(d) = description {
                    write!(f, ": {d}")?;
                }
                Ok(())
            }
            Self::ForbiddenMailFrom => write!(f, "JMAP forbiddenMailFrom"),
            Self::ForbiddenToSend => write!(f, "JMAP forbiddenToSend"),
            Self::CannotUnsendMessage => write!(f, "JMAP cannotUnsendMessage"),
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

impl std::error::Error for JmapMethodError {}
