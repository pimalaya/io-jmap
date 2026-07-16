//! JMAP Mailbox types (RFC 8621 §2).

use core::fmt;

use alloc::{
    string::{String, ToString},
    vec::Vec,
};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

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

/// Client-settable subset of [`JmapMailbox`] for `Mailbox/set` create requests
/// (RFC 8621 §2.1). Server-assigned fields are excluded.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapMailboxCreate {
    /// The user-visible mailbox name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The parent mailbox id; `None` for a top-level mailbox.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    /// The special-use role of the mailbox.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<JmapMailboxRole>,
    /// Position hint for display ordering (lower first).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_order: Option<u32>,
    /// Whether the user is subscribed to the mailbox.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_subscribed: Option<bool>,
}

/// Patch object for `Mailbox/set` update requests (RFC 8620 §5.3): only
/// `Some` fields are serialised.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapMailboxUpdate {
    /// The user-visible mailbox name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The parent mailbox id; `None` for a top-level mailbox.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    /// The special-use role of the mailbox.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<JmapMailboxRole>,
    /// Position hint for display ordering (lower first).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_order: Option<u32>,
    /// Whether the user is subscribed to the mailbox.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_subscribed: Option<bool>,
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

/// Sort property for `Mailbox/query` (RFC 8621 §2.4).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum JmapMailboxSortProperty {
    /// Sort by mailbox name.
    Name,
    /// Sort by the sortOrder position hint.
    SortOrder,
    /// Sort by parent mailbox id.
    ParentId,
}

/// Sort comparator for `Mailbox/query` (RFC 8620 §5.5).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapMailboxSortComparator {
    /// The property to sort by.
    pub property: JmapMailboxSortProperty,
    /// Ascending if `None` or `Some(true)`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_ascending: Option<bool>,
}

/// Filter condition for `Mailbox/query` (RFC 8621 §2.4).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapMailboxFilter {
    /// Filter by parent mailbox ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    /// Filter by role.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<JmapMailboxRole>,
    /// Filter by name (substring match).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Whether to include subscribed mailboxes only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_subscribed: Option<bool>,
    /// Whether to include mailboxes with a role only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_any_role: Option<bool>,
}

/// Per-object error returned in `Mailbox/set` responses (RFC 8621 §2.6).
///
/// Covers the standard RFC 8620 §5.3 set errors plus the mailbox-specific
/// errors defined in RFC 8621 §2.6.
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum JmapMailboxSetItemError {
    /// The mailbox cannot be destroyed because it has child mailboxes.
    MailboxHasChild {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// The mailbox cannot be destroyed because it contains email.
    MailboxHasEmail {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// The referenced object does not exist.
    NotFound {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// The update patch is invalid.
    InvalidPatch {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// The object will be destroyed by this request, so it cannot be updated.
    WillDestroy {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// One or more object properties are invalid.
    InvalidProperties {
        /// Optional human-readable detail.
        description: Option<String>,
        /// The invalid property names.
        #[serde(default)]
        properties: Vec<String>,
    },
    /// The type is a singleton, objects cannot be created or destroyed.
    Singleton {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// Any error type this library does not know about.
    #[serde(other)]
    Unknown,
}
