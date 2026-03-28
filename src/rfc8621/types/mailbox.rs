//! JMAP Mailbox types (RFC 8621 §2).

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A JMAP Mailbox object (RFC 8621 §2.1).
///
/// Mailboxes are named containers for email objects. They correspond
/// to IMAP mailboxes/folders.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Mailbox {
    /// The server-assigned ID for this mailbox.
    pub id: Option<String>,

    /// The display name of this mailbox.
    pub name: Option<String>,

    /// The ID of the parent mailbox, or `null` for a top-level mailbox.
    pub parent_id: Option<String>,

    /// The role of this mailbox (RFC 8621 §2.1).
    pub role: Option<MailboxRole>,

    /// The position of this mailbox in a sorted list of mailboxes.
    #[serde(default)]
    pub sort_order: u32,

    /// Total number of emails in this mailbox.
    #[serde(default)]
    pub total_emails: u32,

    /// Number of unread emails in this mailbox.
    #[serde(default)]
    pub unread_emails: u32,

    /// Total number of threads in this mailbox.
    #[serde(default)]
    pub total_threads: u32,

    /// Number of unread threads in this mailbox.
    #[serde(default)]
    pub unread_threads: u32,

    /// The caller's rights on this mailbox.
    #[serde(default)]
    pub my_rights: MailboxRights,

    /// Whether this mailbox is subscribed.
    #[serde(default)]
    pub is_subscribed: bool,
}

/// A partial [`Mailbox`] object for use in `Mailbox/set` create requests.
///
/// Contains only the client-settable properties (RFC 8621 §2.1).
/// Server-assigned fields (`id`, `totalEmails`, etc.) are excluded.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MailboxCreate {
    /// The display name of this mailbox.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// The ID of the parent mailbox, or `null` for a top-level mailbox.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,

    /// The role of this mailbox.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<MailboxRole>,

    /// The position of this mailbox in a sorted list of mailboxes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_order: Option<u32>,

    /// Whether to subscribe to this mailbox.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_subscribed: Option<bool>,
}

/// A patch object for use in `Mailbox/set` update requests.
///
/// Only the fields that are `Some` are included in the serialized patch,
/// so the server applies only those changes (RFC 8620 §5.3).
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MailboxUpdate {
    /// New display name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// New parent mailbox ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,

    /// New role.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<MailboxRole>,

    /// New sort order.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_order: Option<u32>,

    /// New subscription state.
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

/// A mailbox role as defined in the IANA JMAP Mailbox Roles registry
/// (RFC 8621 §2.1).
///
/// Registered roles are represented by their named variant. Any
/// server-defined or future role not yet in this enum is held by
/// [`MailboxRole::Other`].
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

/// Properties of a [`Mailbox`] object that can be requested in `Mailbox/get`.
///
/// All properties are defined in RFC 8621 §2.1. Pass a subset to
/// `JmapMailboxQuery` to limit the fields returned by the server.
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
