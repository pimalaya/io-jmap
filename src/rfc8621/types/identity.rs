//! JMAP Identity types (RFC 8621 §6).

use serde::{Deserialize, Serialize};

use crate::rfc8621::types::email::EmailAddress;

/// A partial [`Identity`] object for `Identity/set` create requests.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IdentityCreate {
    pub name: String,
    pub email: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<Vec<EmailAddress>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bcc: Option<Vec<EmailAddress>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_signature: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub html_signature: Option<String>,
}

/// Patch object for `Identity/set` update requests.
///
/// Only `Some` fields are serialized.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IdentityUpdate {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<Vec<EmailAddress>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bcc: Option<Vec<EmailAddress>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_signature: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub html_signature: Option<String>,
}

/// A JMAP Identity object (RFC 8621 §6.1).
///
/// An Identity describes a sender identity the user can send email
/// from (name, email address, signature, etc.).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Identity {
    /// The server-assigned ID.
    pub id: String,

    /// The display name for the sender.
    pub name: String,

    /// The email address for the sender.
    pub email: String,

    /// `Reply-To` addresses to set on outgoing email.
    pub reply_to: Option<Vec<EmailAddress>>,

    /// `Bcc` addresses to add to all outgoing email.
    pub bcc: Option<Vec<EmailAddress>>,

    /// Plaintext signature to append to outgoing email.
    pub text_signature: Option<String>,

    /// HTML signature to append to outgoing email.
    pub html_signature: Option<String>,

    /// Whether the user may delete this identity.
    #[serde(default)]
    pub may_delete: bool,
}
