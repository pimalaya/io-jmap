//! JMAP Thread types (RFC 8621 §3).

use serde::{Deserialize, Serialize};

/// A JMAP Thread object (RFC 8621 §3.1).
///
/// A Thread is a set of Email objects that share the same root
/// `Message-ID` and in-reply-to chain.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Thread {
    /// The server-assigned ID for this thread.
    pub id: String,

    /// Ordered list of email IDs in this thread, oldest first.
    pub email_ids: Vec<String>,
}
