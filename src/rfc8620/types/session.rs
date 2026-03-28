//! JMAP session object types (RFC 8620 §2).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use url::Url;

/// The JMAP session object returned by the well-known URL (RFC 8620 §2).
///
/// This is returned by `GET /.well-known/jmap` and contains all
/// configuration needed to make JMAP API requests.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapSession {
    /// The username for the authenticated user.
    pub username: String,

    /// Map of account ID to account information.
    pub accounts: HashMap<String, JmapAccountInfo>,

    /// Map of capability URN to the primary account ID for that capability.
    ///
    /// For example: `"urn:ietf:params:jmap:mail" -> "account-id"`.
    pub primary_accounts: HashMap<String, String>,

    /// Map of capability URN to capability-specific configuration.
    pub capabilities: HashMap<String, serde_json::Value>,

    /// The URL to use for all JMAP API requests (POST).
    pub api_url: Url,

    /// URL template for downloading blobs.
    pub download_url: String,

    /// URL for uploading blobs.
    pub upload_url: String,

    /// URL for server-sent event push notifications.
    pub event_source_url: String,

    /// The current state of the session.
    ///
    /// If this changes, the client should re-fetch the session object.
    pub state: String,
}

/// Information about a single JMAP account within a session.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapAccountInfo {
    /// Human-readable display name for the account.
    pub name: String,

    /// Whether this is the primary personal account of the user.
    pub is_personal: bool,

    /// Whether the account is read-only.
    pub is_read_only: bool,

    /// Map of capability URN to capability-specific account configuration.
    pub account_capabilities: HashMap<String, serde_json::Value>,
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

    /// Returns the primary account ID for the JMAP Mail capability
    /// (`urn:ietf:params:jmap:mail`), or an empty string if not present.
    ///
    /// Shorthand for `primary_account_id_for(capabilities::MAIL)`.
    pub fn primary_account_id(&self) -> String {
        self.primary_account_id_for(capabilities::MAIL)
    }
}

/// JMAP capability URNs (RFC 8620 §2).
pub mod capabilities {
    /// Core JMAP capability (RFC 8620).
    pub const CORE: &str = "urn:ietf:params:jmap:core";
    /// JMAP for Mail capability (RFC 8621).
    pub const MAIL: &str = "urn:ietf:params:jmap:mail";
    /// JMAP for Mail Submission capability (RFC 8621).
    pub const SUBMISSION: &str = "urn:ietf:params:jmap:submission";
    /// JMAP for Vacation Response capability (RFC 8621).
    pub const VACATION_RESPONSE: &str = "urn:ietf:params:jmap:vacationresponse";
}
