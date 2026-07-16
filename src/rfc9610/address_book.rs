//! JMAP for Contacts: AddressBook (RFC 9610 §2).

use alloc::{collections::BTreeMap, string::String};

use serde::{Deserialize, Serialize};

pub mod changes;
pub mod get;
pub mod set;

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
