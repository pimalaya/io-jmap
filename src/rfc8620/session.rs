//! JMAP session object types (RFC 8620 §2).

use alloc::{collections::BTreeMap, string::String};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::Url;

/// The JMAP session object returned by the well-known URL (RFC 8620 §2).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapSession {
    pub username: String,
    pub accounts: BTreeMap<String, JmapAccountInfo>,
    pub primary_accounts: BTreeMap<String, String>,
    pub capabilities: BTreeMap<String, Value>,
    pub api_url: Url,
    pub download_url: String,
    pub upload_url: String,
    pub event_source_url: String,
    pub state: String,
}

/// Information about a single JMAP account within a session.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapAccountInfo {
    pub name: String,
    pub is_personal: bool,
    pub is_read_only: bool,
    pub account_capabilities: BTreeMap<String, Value>,
}

impl JmapSession {
    /// Returns the primary account ID for the given capability URN, or an
    /// empty string if none is advertised.
    pub fn primary_account_id_for(&self, capability: &str) -> String {
        self.primary_accounts
            .get(capability)
            .cloned()
            .unwrap_or_default()
    }
}

/// JMAP capability URNs (RFC 8620 §2).
pub mod capabilities {
    /// Core JMAP capability (RFC 8620).
    pub const CORE: &str = "urn:ietf:params:jmap:core";
}
