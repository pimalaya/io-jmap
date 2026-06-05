//! JMAP Mailbox types (RFC 8621 §2).

use alloc::{
    string::{String, ToString},
    vec::Vec,
};
use core::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A JMAP Mailbox object (RFC 8621 §2.1): a named container for emails.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Mailbox {
    pub id: Option<String>,
    pub name: Option<String>,
    /// `None` for a top-level mailbox.
    pub parent_id: Option<String>,
    pub role: Option<MailboxRole>,
    #[serde(default)]
    pub sort_order: u32,
    #[serde(default)]
    pub total_emails: u32,
    #[serde(default)]
    pub unread_emails: u32,
    #[serde(default)]
    pub total_threads: u32,
    #[serde(default)]
    pub unread_threads: u32,
    #[serde(default)]
    pub my_rights: MailboxRights,
    #[serde(default)]
    pub is_subscribed: bool,
}

/// Client-settable subset of [`Mailbox`] for `Mailbox/set` create requests
/// (RFC 8621 §2.1). Server-assigned fields are excluded.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MailboxCreate {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<MailboxRole>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_order: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_subscribed: Option<bool>,
}

/// Patch object for `Mailbox/set` update requests (RFC 8620 §5.3): only
/// `Some` fields are serialised.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MailboxUpdate {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<MailboxRole>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_order: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_subscribed: Option<bool>,
}

/// Access rights on a mailbox (RFC 8621 §2.1).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MailboxRights {
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
/// Any unknown or server-defined role is held by [`MailboxRole::Other`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MailboxRole {
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

impl fmt::Display for MailboxRole {
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

impl Serialize for MailboxRole {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for MailboxRole {
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

/// [`Mailbox`] properties requestable in `Mailbox/get` (RFC 8621 §2.1).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MailboxProperty {
    Id,
    Name,
    ParentId,
    Role,
    SortOrder,
    TotalEmails,
    UnreadEmails,
    TotalThreads,
    UnreadThreads,
    MyRights,
    IsSubscribed,
}

/// Sort property for `Mailbox/query` (RFC 8621 §2.4).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MailboxSortProperty {
    Name,
    SortOrder,
    ParentId,
}

/// Sort comparator for `Mailbox/query` (RFC 8620 §5.5).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MailboxSortComparator {
    pub property: MailboxSortProperty,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_ascending: Option<bool>,
}

/// Filter for `Mailbox/query` (RFC 8621 §2.4).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MailboxFilter {
    /// Filter by parent mailbox ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,

    /// Filter by role.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<MailboxRole>,

    /// Filter by name (substring match).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// Whether to include subscribed mailboxes only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_subscribed: Option<bool>,

    /// Whether to include mailboxes with unread email only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_any_role: Option<bool>,
}

/// Per-object error returned in `Mailbox/set` responses (RFC 8621 §2.6).
///
/// Covers the standard RFC 8620 §5.3 set errors plus the mailbox-specific
/// errors defined in RFC 8621 §2.6.
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum MailboxSetError {
    /// The mailbox cannot be destroyed because it has child mailboxes.
    MailboxHasChild {
        description: Option<String>,
    },
    /// The mailbox cannot be destroyed because it contains email.
    MailboxHasEmail {
        description: Option<String>,
    },
    NotFound {
        description: Option<String>,
    },
    InvalidPatch {
        description: Option<String>,
    },
    WillDestroy {
        description: Option<String>,
    },
    InvalidProperties {
        description: Option<String>,
        #[serde(default)]
        properties: Vec<String>,
    },
    Singleton {
        description: Option<String>,
    },
    #[serde(other)]
    Unknown,
}
