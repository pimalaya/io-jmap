//! RFC 8621: JMAP for Mail.

use alloc::{string::String, vec::Vec};

use serde::Deserialize;

use crate::rfc8620::session::JmapSession;

pub mod email;
pub mod email_submission;
pub mod identity;
pub mod mailbox;
pub mod thread;
pub mod vacation_response;

/// JMAP for Mail capability (RFC 8621).
pub const JMAP_MAIL_CAPABILITY: &str = "urn:ietf:params:jmap:mail";

/// Value of the mail capability in an account's `accountCapabilities` (RFC
/// 8621 §1.3.1): the account's mail limits and permissions, each `None`
/// when the server left it out.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapMailCapability {
    /// Max number of mailboxes one email may belong to; `None` for no
    /// limit.
    #[serde(default)]
    pub max_mailboxes_per_email: Option<u64>,
    /// Max depth of the mailbox hierarchy; `None` for no limit.
    #[serde(default)]
    pub max_mailbox_depth: Option<u64>,
    /// Max size of a mailbox name, in octets.
    #[serde(default)]
    pub max_size_mailbox_name: Option<u64>,
    /// Max total size of the attachments of one email, in octets.
    #[serde(default)]
    pub max_size_attachments_per_email: Option<u64>,
    /// The `Email/query` sort properties the server supports.
    #[serde(default)]
    pub email_query_sort_options: Vec<String>,
    /// Whether the user may create a mailbox at the top level.
    #[serde(default)]
    pub may_create_top_level_mailbox: Option<bool>,
}

impl JmapSession {
    /// Returns the mail capability of the given account (RFC 8621
    /// §1.3.1), every field `None` when the account is unknown or the
    /// capability absent or unreadable.
    ///
    /// The session-level mail capability is an empty object, the limits
    /// live in the account's `accountCapabilities`.
    pub fn mail_capability(&self, account_id: &str) -> JmapMailCapability {
        self.accounts
            .get(account_id)
            .and_then(|account| account.account_capabilities.get(JMAP_MAIL_CAPABILITY))
            .and_then(|mail| serde_json::from_value(mail.clone()).ok())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use crate::rfc8621::*;

    fn session(account_capabilities: Value) -> JmapSession {
        serde_json::from_value(json!({
            "username": "",
            "accounts": {
                "a1": {
                    "name": "",
                    "isPersonal": true,
                    "isReadOnly": false,
                    "accountCapabilities": account_capabilities,
                },
            },
            "primaryAccounts": {"urn:ietf:params:jmap:mail": "a1"},
            "capabilities": {"urn:ietf:params:jmap:mail": {}},
            "apiUrl": "https://api.example.com/jmap/",
            "downloadUrl": "",
            "uploadUrl": "",
            "eventSourceUrl": "",
            "state": "s1",
        }))
        .unwrap()
    }

    #[test]
    fn mail_limits_read_from_the_account_capability() {
        let session = session(json!({
            "urn:ietf:params:jmap:mail": {
                "maxMailboxesPerEmail": null,
                "maxMailboxDepth": 10,
                "maxSizeMailboxName": 490,
                "maxSizeAttachmentsPerEmail": 50000000,
                "emailQuerySortOptions": ["receivedAt", "sentAt"],
                "mayCreateTopLevelMailbox": true,
            },
        }));

        let mail = session.mail_capability("a1");

        assert_eq!(mail.max_mailboxes_per_email, None);
        assert_eq!(mail.max_mailbox_depth, Some(10));
        assert_eq!(mail.max_size_mailbox_name, Some(490));
        assert_eq!(mail.max_size_attachments_per_email, Some(50_000_000));
        assert_eq!(mail.email_query_sort_options, ["receivedAt", "sentAt"]);
        assert_eq!(mail.may_create_top_level_mailbox, Some(true));
    }

    #[test]
    fn no_mail_capability_reads_as_unknown() {
        let session = session(json!({}));

        let mail = session.mail_capability("a1");
        assert_eq!(mail.max_mailbox_depth, None);
        assert!(mail.email_query_sort_options.is_empty());
        assert_eq!(mail.may_create_top_level_mailbox, None);

        let unknown = session.mail_capability("a2");
        assert_eq!(unknown.max_size_mailbox_name, None);
    }
}
