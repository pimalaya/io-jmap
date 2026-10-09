//! JMAP for Mail: EmailSubmission (RFC 8621 §7).

use core::fmt;

use alloc::{collections::BTreeMap, string::String, vec::Vec};

use serde::{Deserialize, Serialize};

use crate::rfc8620::error::{write_described, write_properties};

pub mod cancel;
pub mod get;
pub mod query;
pub mod set;

/// JMAP for Mail Submission capability (RFC 8621 §7).
pub const JMAP_SUBMISSION_CAPABILITY: &str = "urn:ietf:params:jmap:submission";

/// The undo status of an email submission (RFC 8621 §7.1).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum JmapUndoStatus {
    /// The submission may still be cancelled.
    Pending,
    /// The submission can no longer be cancelled.
    Final,
    /// The submission was cancelled.
    Canceled,
}

impl fmt::Display for JmapUndoStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Final => write!(f, "final"),
            Self::Canceled => write!(f, "canceled"),
        }
    }
}

/// A JMAP EmailSubmission object (RFC 8621 §7.1).
///
/// Represents a sending of an email from a particular identity.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct JmapEmailSubmission {
    /// Server-assigned ID.
    pub id: Option<String>,
    /// The identity to send as.
    pub identity_id: Option<String>,
    /// The ID of the email to send.
    pub email_id: Option<String>,
    /// The thread the email belongs to.
    pub thread_id: Option<String>,
    /// SMTP envelope to use for delivery.
    pub envelope: Option<JmapEnvelope>,
    /// Date/time the submission was made (RFC 3339).
    pub send_at: Option<String>,
    /// Current undo status: `"pending"`, `"final"`, or `"canceled"`.
    pub undo_status: Option<JmapUndoStatus>,
    /// Per-recipient delivery status.
    pub delivery_status: Option<BTreeMap<String, JmapDeliveryStatus>>,
    /// Blob IDs of DSN messages.
    pub dsn_blob_ids: Option<Vec<String>>,
    /// Blob IDs of MDN messages.
    pub mdn_blob_ids: Option<Vec<String>>,
}

/// SMTP envelope for an email submission.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct JmapEnvelope {
    /// MAIL FROM address and parameters.
    pub mail_from: JmapEmailAddressWithParameters,
    /// RCPT TO addresses and parameters.
    pub rcpt_to: Vec<JmapEmailAddressWithParameters>,
}

/// An email address with optional SMTP parameters.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct JmapEmailAddressWithParameters {
    /// The email address.
    pub email: String,
    /// SMTP parameters (e.g. `NOTIFY`, `ORCPT`).
    pub parameters: Option<BTreeMap<String, Option<String>>>,
}

/// Delivery state of a single recipient (RFC 8621 §7.1.1).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum JmapDelivered {
    /// The message is in a local mail queue.
    Queued,
    /// The message was successfully delivered.
    Yes,
    /// Delivery failed permanently.
    No,
    /// The delivery status is unknown.
    Unknown,
}

/// Whether the email has been displayed to the recipient (RFC 8621 §7.1.1).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum JmapDisplayed {
    /// Display status is unknown.
    Unknown,
    /// The message has been displayed.
    Yes,
    /// The message has not been displayed.
    No,
}

/// Per-recipient delivery status from a submission.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct JmapDeliveryStatus {
    /// The SMTP reply for this recipient.
    pub smtp_reply: String,
    /// Delivery state for this recipient.
    pub delivered: JmapDelivered,
    /// Whether the message has been displayed to the recipient.
    pub displayed: JmapDisplayed,
}

/// Per-object error returned in `EmailSubmission/set` responses
/// (RFC 8621 §7.5); shared by the create (`set`) and cancel flows.
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum JmapEmailSubmissionSetItemError {
    /// The message had too many recipients (RFC 8621 §7.5).
    TooManyRecipients {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// The message had no recipients (RFC 8621 §7.5).
    NoRecipients {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// One or more recipient addresses were invalid (RFC 8621 §7.5).
    InvalidRecipients {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// The From address is not permitted for this identity (RFC 8621 §7.5).
    ForbiddenFrom {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// The MAIL FROM address is not permitted (RFC 8621 §7.5).
    ForbiddenMailFrom {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// This user is not permitted to send email (RFC 8621 §7.5).
    ForbiddenToSend {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// The submission cannot be unsent (RFC 8621 §7.5).
    #[serde(alias = "cannotUnsend")]
    CannotUnsendMessage {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// The email object was not a valid message (RFC 8621 §7.5).
    InvalidEmail {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// The email is larger than the server sends (RFC 8621 §7.5).
    TooLarge {
        /// Optional human-readable detail.
        description: Option<String>,
        /// The largest email the server sends, in octets.
        #[serde(default, rename = "maxSize")]
        max_size: Option<u64>,
    },
    /// Standard set error (RFC 8620 §5.3): the submission is not allowed.
    Forbidden {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// Standard set error (RFC 8620 §5.3): the submission would exceed a
    /// quota.
    OverQuota {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// Standard set error (RFC 8620 §5.3): too many submissions lately,
    /// to try again later.
    RateLimit {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// Standard set error (RFC 8620 §5.3): target id not found.
    NotFound {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// Standard set error (RFC 8620 §5.3): one or more properties were invalid.
    InvalidProperties {
        /// Optional human-readable detail.
        description: Option<String>,
        /// The invalid property names.
        #[serde(default)]
        properties: Vec<String>,
    },
    /// Catch-all for set errors not modelled above.
    #[serde(other)]
    Unknown,
}

impl fmt::Display for JmapEmailSubmissionSetItemError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooManyRecipients { description } => {
                write_described(f, "tooManyRecipients", description)
            }
            Self::NoRecipients { description } => write_described(f, "noRecipients", description),
            Self::InvalidRecipients { description } => {
                write_described(f, "invalidRecipients", description)
            }
            Self::ForbiddenFrom { description } => write_described(f, "forbiddenFrom", description),
            Self::ForbiddenMailFrom { description } => {
                write_described(f, "forbiddenMailFrom", description)
            }
            Self::ForbiddenToSend { description } => {
                write_described(f, "forbiddenToSend", description)
            }
            Self::CannotUnsendMessage { description } => {
                write_described(f, "cannotUnsend", description)
            }
            Self::InvalidEmail { description } => write_described(f, "invalidEmail", description),
            Self::TooLarge {
                description,
                max_size,
            } => {
                write_described(f, "tooLarge", description)?;
                match max_size {
                    Some(max) => write!(f, " (at most {max} octets)"),
                    None => Ok(()),
                }
            }
            Self::Forbidden { description } => write_described(f, "forbidden", description),
            Self::OverQuota { description } => write_described(f, "overQuota", description),
            Self::RateLimit { description } => write_described(f, "rateLimit", description),
            Self::NotFound { description } => write_described(f, "notFound", description),
            Self::InvalidProperties {
                description,
                properties,
            } => write_properties(f, description, properties),
            Self::Unknown => write!(f, "JMAP unknown error"),
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::string::ToString;

    use serde_json::{from_value, json};

    use super::JmapEmailSubmissionSetItemError;

    #[test]
    fn a_refusal_reads_as_its_type_and_description() {
        let refused: JmapEmailSubmissionSetItemError = from_value(json!({
            "type": "forbiddenFrom",
            "description": "Not your address",
        }))
        .unwrap();

        assert_eq!(refused.to_string(), "JMAP forbiddenFrom: Not your address");
    }

    #[test]
    fn too_large_keeps_the_size_the_server_sends() {
        let refused: JmapEmailSubmissionSetItemError =
            from_value(json!({ "type": "tooLarge", "maxSize": 1000 })).unwrap();

        assert!(matches!(
            refused,
            JmapEmailSubmissionSetItemError::TooLarge {
                max_size: Some(1000),
                ..
            }
        ));
        assert_eq!(refused.to_string(), "JMAP tooLarge (at most 1000 octets)");
    }

    #[test]
    fn the_rfc_spelling_of_cannot_unsend_is_read() {
        let refused: JmapEmailSubmissionSetItemError =
            from_value(json!({ "type": "cannotUnsend" })).unwrap();

        assert!(matches!(
            refused,
            JmapEmailSubmissionSetItemError::CannotUnsendMessage { .. }
        ));
    }
}
