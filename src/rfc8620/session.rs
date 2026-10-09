//! JMAP session object (RFC 8620 §2): the account map and capability set
//! returned by the well-known session URL.

use core::fmt::Write;

use alloc::{collections::BTreeMap, string::String};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::{ParseError, Url};

use crate::rfc8620::{JMAP_CORE_CAPABILITY, JmapCoreCapability};

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

    /// Returns the server's core limits (RFC 8620 §2), every one `None`
    /// when the core capability is absent or unreadable.
    pub fn core_capability(&self) -> JmapCoreCapability {
        self.capabilities
            .get(JMAP_CORE_CAPABILITY)
            .and_then(|core| serde_json::from_value(core.clone()).ok())
            .unwrap_or_default()
    }

    /// Resolves the `uploadUrl` template (RFC 8620 §6.1) for one
    /// account.
    pub fn resolve_upload_url(&self, account_id: &str) -> Result<Url, ParseError> {
        Url::parse(&self.upload_url.replace("{accountId}", &encode(account_id)))
    }

    /// Resolves the `downloadUrl` template (RFC 8620 §6.2) for one blob
    /// of one account, offered under `name` as the media type `type_`.
    pub fn resolve_download_url(
        &self,
        account_id: &str,
        blob_id: &str,
        name: &str,
        type_: &str,
    ) -> Result<Url, ParseError> {
        let filled = self
            .download_url
            .replace("{accountId}", &encode(account_id))
            .replace("{blobId}", &encode(blob_id))
            .replace("{name}", &encode(name))
            .replace("{type}", &encode(type_));
        Url::parse(&filled)
    }
}

/// One RFC 6570 template variable as a URL component: everything outside
/// the RFC 3986 unreserved set percent-encoded, an id being opaque and
/// free to carry a `/`.
fn encode(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                encoded.push(char::from(byte))
            }
            _ => {
                let _ = write!(encoded, "%{byte:02X}");
            }
        }
    }
    encoded
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn session(capabilities: Value) -> JmapSession {
        serde_json::from_value(json!({
            "username": "",
            "accounts": {},
            "primaryAccounts": {},
            "capabilities": capabilities,
            "apiUrl": "https://api.example.com/jmap/",
            "downloadUrl": "https://api.example.com/d/{accountId}/{blobId}/{name}?accept={type}",
            "uploadUrl": "https://api.example.com/u/{accountId}/",
            "eventSourceUrl": "",
            "state": "s1",
        }))
        .unwrap()
    }

    #[test]
    fn core_limits_read_from_the_capability() {
        let session = session(json!({
            "urn:ietf:params:jmap:core": {
                "maxSizeUpload": 50000000,
                "maxObjectsInGet": 1000,
                "collationAlgorithms": ["i;ascii-casemap"],
            },
        }));

        let core = session.core_capability();

        assert_eq!(core.max_size_upload, Some(50_000_000));
        assert_eq!(core.max_objects_in_get, Some(1000));
        assert_eq!(core.max_objects_in_set, None);
        assert_eq!(core.collation_algorithms, ["i;ascii-casemap"]);
    }

    #[test]
    fn no_core_capability_reads_as_no_limit() {
        let core = session(json!({})).core_capability();

        assert_eq!(core.max_objects_in_get, None);
        assert!(core.collation_algorithms.is_empty());
    }

    #[test]
    fn templates_leave_no_variable_and_encode_ids() {
        let session = session(json!({}));

        let upload = session.resolve_upload_url("u42").unwrap();
        let download = session
            .resolve_download_url("u42", "a/b c", "message.eml", "message/rfc822")
            .unwrap();

        assert_eq!(upload.as_str(), "https://api.example.com/u/u42/");
        assert_eq!(
            download.as_str(),
            "https://api.example.com/d/u42/a%2Fb%20c/message.eml?accept=message%2Frfc822"
        );
    }
}
