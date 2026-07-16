//! JMAP for Mail: Mailbox (RFC 8621 §2).

use core::fmt;

use alloc::string::{String, ToString};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub mod changes;
pub mod get;
pub mod query;
pub mod set;

/// A JMAP Mailbox object (RFC 8621 §2.1): a named container for emails.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapMailbox {
    /// The server-assigned mailbox id.
    pub id: Option<String>,
    /// The user-visible mailbox name.
    pub name: Option<String>,
    /// `None` for a top-level mailbox.
    pub parent_id: Option<String>,
    /// The special-use role of the mailbox, when any.
    pub role: Option<JmapMailboxRole>,
    /// Position hint for display ordering (lower first).
    #[serde(default)]
    pub sort_order: u32,
    /// The number of emails in the mailbox.
    #[serde(default)]
    pub total_emails: u32,
    /// The number of unread emails in the mailbox.
    #[serde(default)]
    pub unread_emails: u32,
    /// The number of threads with at least one email in the mailbox.
    #[serde(default)]
    pub total_threads: u32,
    /// The number of threads with at least one unread email in the mailbox.
    #[serde(default)]
    pub unread_threads: u32,
    /// The user's rights on the mailbox.
    #[serde(default)]
    pub my_rights: JmapMailboxRights,
    /// Whether the user is subscribed to the mailbox.
    #[serde(default)]
    pub is_subscribed: bool,
}

/// Access rights on a mailbox (RFC 8621 §2.1).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapMailboxRights {
    /// May read items in the mailbox.
    pub may_read_items: bool,
    /// May add items to the mailbox.
    pub may_add_items: bool,
    /// May remove items from the mailbox.
    pub may_remove_items: bool,
    /// May set/unset the `$seen` keyword on items.
    pub may_set_seen: bool,
    /// May set/unset any keyword other than `$seen`.
    pub may_set_keywords: bool,
    /// May create child mailboxes.
    pub may_create_child: bool,
    /// May rename this mailbox.
    pub may_rename: bool,
    /// May delete this mailbox.
    pub may_delete: bool,
    /// May submit email from this mailbox.
    pub may_submit: bool,
}

/// Mailbox role per the IANA JMAP Mailbox Roles registry (RFC 8621 §2.1).
/// Any unknown or server-defined role is held by [`JmapMailboxRole::Other`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JmapMailboxRole {
    /// Primary inbox.
    Inbox,
    /// Archived messages.
    Archive,
    /// Draft messages.
    Drafts,
    /// Flagged / starred messages.
    Flagged,
    /// Messages marked as important.
    Important,
    /// Spam / junk messages.
    Junk,
    /// Sent messages.
    Sent,
    /// Virtual mailbox of all subscribed mailboxes.
    Subscribed,
    /// Deleted messages.
    Trash,
    /// A server-defined or unrecognised role.
    Other(String),
}

impl fmt::Display for JmapMailboxRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Inbox => "inbox",
            Self::Archive => "archive",
            Self::Drafts => "drafts",
            Self::Flagged => "flagged",
            Self::Important => "important",
            Self::Junk => "junk",
            Self::Sent => "sent",
            Self::Subscribed => "subscribed",
            Self::Trash => "trash",
            Self::Other(s) => s.as_str(),
        })
    }
}

impl Serialize for JmapMailboxRole {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for JmapMailboxRole {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Ok(match s.as_str() {
            "inbox" => Self::Inbox,
            "archive" => Self::Archive,
            "drafts" => Self::Drafts,
            "flagged" => Self::Flagged,
            "important" => Self::Important,
            "junk" => Self::Junk,
            "sent" => Self::Sent,
            "subscribed" => Self::Subscribed,
            "trash" => Self::Trash,
            _ => Self::Other(s),
        })
    }
}

/// [`JmapMailbox`] properties requestable in `Mailbox/get` (RFC 8621 §2.1).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum JmapMailboxProperty {
    /// The `id` property.
    Id,
    /// The `name` property.
    Name,
    /// The `parentId` property.
    ParentId,
    /// The `role` property.
    Role,
    /// The `sortOrder` property.
    SortOrder,
    /// The `totalEmails` property.
    TotalEmails,
    /// The `unreadEmails` property.
    UnreadEmails,
    /// The `totalThreads` property.
    TotalThreads,
    /// The `unreadThreads` property.
    UnreadThreads,
    /// The `myRights` property.
    MyRights,
    /// The `isSubscribed` property.
    IsSubscribed,
}
