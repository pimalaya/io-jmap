//! JMAP for Mail: EmailSubmission (RFC 8621 §7).

use core::fmt;

use alloc::{collections::BTreeMap, string::String, vec::Vec};

use serde::{Deserialize, Serialize};

pub mod cancel;
pub mod get;
pub mod query;
pub mod set;

/// JMAP for Mail Submission capability (RFC 8621 §7).
pub const JMAP_SUBMISSION_CAPABILITY: &str = "urn:ietf:params:jmap:submission";

/// The undo status of an email submission (RFC 8621 §7.1).
#[derive(Clone, Debug, Serialize, Deserialize)]
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
#[serde(rename_all = "camelCase")]
pub struct JmapEnvelope {
    /// MAIL FROM address and parameters.
    pub mail_from: JmapEmailAddressWithParameters,
    /// RCPT TO addresses and parameters.
    pub rcpt_to: Vec<JmapEmailAddressWithParameters>,
}

/// An email address with optional SMTP parameters.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapEmailAddressWithParameters {
    /// The email address.
    pub email: String,
    /// SMTP parameters (e.g. `NOTIFY`, `ORCPT`).
    pub parameters: Option<BTreeMap<String, Option<String>>>,
}

/// Delivery state of a single recipient (RFC 8621 §7.1.1).
#[derive(Clone, Debug, Serialize, Deserialize)]
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
    CannotUnsendMessage {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// The email object was not a valid message (RFC 8621 §7.5).
    InvalidEmail {
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
