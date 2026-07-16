//! JMAP for Mail: Identity (RFC 8621 §6).

use alloc::{string::String, vec::Vec};

use serde::{Deserialize, Serialize};

use crate::rfc8621::email::JmapEmailAddress;

pub mod get;
pub mod set;

/// A JMAP Identity object (RFC 8621 §6.1).
///
/// An Identity describes a sender identity the user can send email
/// from (name, email address, signature, etc.).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapIdentity {
    /// The server-assigned ID.
    pub id: String,
    /// The display name for the sender.
    pub name: String,
    /// The email address for the sender.
    pub email: String,
    /// `Reply-To` addresses to set on outgoing email.
    pub reply_to: Option<Vec<JmapEmailAddress>>,
    /// `Bcc` addresses to add to all outgoing email.
    pub bcc: Option<Vec<JmapEmailAddress>>,
    /// Plaintext signature to append to outgoing email.
    pub text_signature: Option<String>,
    /// HTML signature to append to outgoing email.
    pub html_signature: Option<String>,
    /// Whether the user may delete this identity.
    #[serde(default)]
    pub may_delete: bool,
}
