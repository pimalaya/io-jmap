//! JMAP EmailSubmission types (RFC 8621 §7).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// A JMAP EmailSubmission object (RFC 8621 §7.1).
///
/// Represents a sending of an email from a particular identity.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmailSubmission {
    /// Server-assigned ID.
    pub id: Option<String>,

    /// The identity to send as.
    pub identity_id: String,

    /// The ID of the Email to send.
    pub email_id: String,

    /// The thread the email belongs to.
    pub thread_id: Option<String>,

    /// SMTP envelope to use for delivery.
    pub envelope: Option<Envelope>,

    /// Date/time the submission was made (RFC 3339).
    pub send_at: Option<String>,

    /// Current undo status: `"pending"`, `"final"`, or `"canceled"`.
    pub undo_status: Option<String>,

    /// Per-recipient delivery status.
    pub delivery_status: Option<HashMap<String, DeliveryStatus>>,

    /// Blob IDs of DSN messages.
    pub dsn_blob_ids: Option<Vec<String>>,

    /// Blob IDs of MDN messages.
    pub mdn_blob_ids: Option<Vec<String>>,
}

/// SMTP envelope for an email submission.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Envelope {
    /// MAIL FROM address and parameters.
    pub mail_from: EmailAddressWithParameters,

    /// RCPT TO addresses and parameters.
    pub rcpt_to: Vec<EmailAddressWithParameters>,
}

/// An email address with optional SMTP parameters.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmailAddressWithParameters {
    /// The email address.
    pub email: String,

    /// SMTP parameters (e.g. `NOTIFY`, `ORCPT`).
    pub parameters: Option<HashMap<String, Option<String>>>,
}

/// Per-recipient delivery status from a submission.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryStatus {
    /// The SMTP reply for this recipient.
    pub smtp_reply: String,

    /// Delivery status: `"queued"`, `"yes"`, `"no"`, `"unknown"`.
    pub delivered: String,

    /// Display status: `"unknown"`, `"yes"`, `"no"`.
    pub displayed: String,
}

/// A single email submission to create via `EmailSubmission/set`.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmailSubmissionCreate {
    /// The identity to send as.
    pub identity_id: String,

    /// The ID of the Email to send.
    pub email_id: String,

    /// SMTP envelope override (uses email headers if omitted).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub envelope: Option<Envelope>,
}

/// Patch object for `EmailSubmission/set` update.
///
/// Only `undoStatus` can be updated (to `"canceled"`).
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmailSubmissionUpdate {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub undo_status: Option<String>,
}

/// Filter for `EmailSubmission/query` (RFC 8621 §7.4).
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmailSubmissionFilter {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity_ids: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub email_ids: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_ids: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub undo_status: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
}

/// Sort property for `EmailSubmission/query`.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum EmailSubmissionSortProperty {
    EmailId,
    ThreadId,
    SentAt,
}

/// Sort comparator for `EmailSubmission/query`.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmailSubmissionComparator {
    pub property: EmailSubmissionSortProperty,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_ascending: Option<bool>,
}
