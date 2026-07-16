//! JMAP AddressBook types (RFC 9610 §2).

use alloc::{collections::BTreeMap, string::String, vec::Vec};

use serde::{Deserialize, Serialize};

/// A JMAP AddressBook object (RFC 9610 §2): a named collection of
/// ContactCards.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapAddressBook {
    /// The server-assigned address book id.
    pub id: Option<String>,
    /// The user-visible address book name.
    pub name: Option<String>,
    /// Optional long-form description providing context in shared
    /// environments.
    pub description: Option<String>,
    /// Position hint for display ordering (lower first).
    #[serde(default)]
    pub sort_order: u32,
    /// True for at most one AddressBook per account (server-set).
    #[serde(default)]
    pub is_default: bool,
    /// Whether the user is subscribed to the address book.
    #[serde(default)]
    pub is_subscribed: bool,
    /// Principal id to rights map (RFC 9670); `None` when unshared or when
    /// the server does not support sharing.
    #[serde(default)]
    pub share_with: Option<BTreeMap<String, JmapAddressBookRights>>,
    /// The user's rights on the address book.
    #[serde(default)]
    pub my_rights: JmapAddressBookRights,
}

/// Client-settable subset of [`JmapAddressBook`] for `AddressBook/set`
/// create requests (RFC 9610 §2). Server-assigned fields are excluded.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapAddressBookCreate {
    /// The user-visible address book name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Optional long-form description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Position hint for display ordering (lower first).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_order: Option<u32>,
    /// Whether the user is subscribed to the address book.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_subscribed: Option<bool>,
    /// Principal id to rights map (RFC 9670).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub share_with: Option<BTreeMap<String, JmapAddressBookRights>>,
}

/// Patch object for `AddressBook/set` update requests (RFC 8620 §5.3): only
/// `Some` fields are serialised.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapAddressBookUpdate {
    /// The user-visible address book name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Optional long-form description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Position hint for display ordering (lower first).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_order: Option<u32>,
    /// Whether the user is subscribed to the address book.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_subscribed: Option<bool>,
    /// Principal id to rights map (RFC 9670).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub share_with: Option<BTreeMap<String, JmapAddressBookRights>>,
}

/// Access rights on an AddressBook (RFC 9610 §2).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapAddressBookRights {
    /// May fetch the ContactCards in this AddressBook.
    pub may_read: bool,
    /// May create, modify, or destroy ContactCards in this AddressBook, or
    /// move them to or from it.
    pub may_write: bool,
    /// May modify the `shareWith` property of this AddressBook.
    pub may_share: bool,
    /// May delete the AddressBook itself.
    pub may_delete: bool,
}

/// [`JmapAddressBook`] properties requestable in `AddressBook/get`
/// (RFC 9610 §2).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum JmapAddressBookProperty {
    /// The `id` property.
    Id,
    /// The `name` property.
    Name,
    /// The `description` property.
    Description,
    /// The `sortOrder` property.
    SortOrder,
    /// The `isDefault` property.
    IsDefault,
    /// The `isSubscribed` property.
    IsSubscribed,
    /// The `shareWith` property.
    ShareWith,
    /// The `myRights` property.
    MyRights,
}

/// Per-object error returned in `AddressBook/set` responses (RFC 9610 §2.3).
///
/// Covers the standard RFC 8620 §5.3 set errors plus the AddressBook-specific
/// error defined in RFC 9610 §2.3.
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum JmapAddressBookSetItemError {
    /// The AddressBook still has ContactCards and `onDestroyRemoveContents`
    /// was false (RFC 9610 §2.3).
    AddressBookHasContents {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// Standard set error (RFC 8620 §5.3): the change is not allowed, e.g.
    /// a `shareWith` or `isSubscribed` change rejected by the server.
    Forbidden {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// Standard set error (RFC 8620 §5.3): target id not found.
    NotFound {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// Standard set error (RFC 8620 §5.3): patch could not be applied.
    InvalidPatch {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// Standard set error (RFC 8620 §5.3): would destroy an object already
    /// queued for destruction in the same request.
    WillDestroy {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// Standard set error (RFC 8620 §5.3): one or more properties were
    /// invalid.
    InvalidProperties {
        /// Optional human-readable detail.
        description: Option<String>,
        /// The invalid property names.
        #[serde(default)]
        properties: Vec<String>,
    },
    /// Catch-all for set errors not modelled above.
    #[serde(other)]
    Unknown,
}
