//! JMAP session object (RFC 8620 §2): the account map and capability set
//! returned by the well-known session URL.

use alloc::{collections::BTreeMap, string::String};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::Url;

/// The JMAP session object returned by the well-known URL (RFC 8620 §2).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapSession {
    /// The username associated with the credentials used to fetch the
    /// session.
    pub username: String,
    /// The accounts the user has access to, keyed by account id.
    pub accounts: BTreeMap<String, JmapAccountInfo>,
    /// The primary account id per capability URN.
    pub primary_accounts: BTreeMap<String, String>,
    /// The capabilities the server supports, keyed by capability URN.
    pub capabilities: BTreeMap<String, Value>,
    /// The URL to POST JMAP API requests to.
    pub api_url: Url,
    /// The blob download URL template (RFC 6570).
    pub download_url: String,
    /// The blob upload URL template (RFC 6570).
    pub upload_url: String,
    /// The URL of the event source push channel.
    pub event_source_url: String,
    /// The opaque server state; changes when the session object changes.
    pub state: String,
}

impl JmapSession {
    /// Returns the primary account ID for the given capability URN, or an empty
    /// string if none is advertised.
    pub fn primary_account_id_for(&self, capability: &str) -> String {
        self.primary_accounts
            .get(capability)
            .cloned()
            .unwrap_or_default()
    }
}

/// Information about a single JMAP account within a session.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapAccountInfo {
    /// The human-readable account name.
    pub name: String,
    /// Whether the account belongs to the authenticated user.
    pub is_personal: bool,
    /// Whether the account is read-only.
    pub is_read_only: bool,
    /// Account-level capability objects, keyed by capability URN.
    pub account_capabilities: BTreeMap<String, Value>,
}
