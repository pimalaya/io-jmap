//! JMAP Identity types (RFC 8621 §6).

use alloc::{string::String, vec::Vec};

use serde::{Deserialize, Serialize};

use crate::rfc8621::email::JmapEmailAddress;

/// A partial [`JmapIdentity`] object for `Identity/set` create requests.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapIdentityCreate {
    /// The display name for the sender.
    pub name: String,
    /// The email address for the sender.
    pub email: String,
    /// `Reply-To` addresses to set on outgoing email.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<Vec<JmapEmailAddress>>,
    /// `Bcc` addresses to add to all outgoing email.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bcc: Option<Vec<JmapEmailAddress>>,
    /// Plaintext signature to append to outgoing email.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_signature: Option<String>,
    /// HTML signature to append to outgoing email.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub html_signature: Option<String>,
}

/// Patch object for `Identity/set` update requests.
///
/// Only `Some` fields are serialized.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapIdentityUpdate {
    /// The display name for the sender.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// `Reply-To` addresses to set on outgoing email.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<Vec<JmapEmailAddress>>,
    /// `Bcc` addresses to add to all outgoing email.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bcc: Option<Vec<JmapEmailAddress>>,
    /// Plaintext signature to append to outgoing email.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_signature: Option<String>,
    /// HTML signature to append to outgoing email.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub html_signature: Option<String>,
}

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

/// Per-object error returned in `Identity/set` responses (RFC 8621 §6.4).
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum JmapIdentitySetItemError {
    /// Standard set error (RFC 8620 §5.3): target id not found.
    NotFound {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// Standard set error (RFC 8620 §5.3): patch could not be applied.
    InvalidPatch {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// Standard set error (RFC 8620 §5.3): would destroy an object already
    /// queued for destruction in the same request.
    WillDestroy {
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
    /// Standard set error (RFC 8620 §5.3): tried to create/destroy a
    /// server-managed singleton.
    Singleton {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// Catch-all for set errors not modelled above.
    #[serde(other)]
    Unknown,
}
