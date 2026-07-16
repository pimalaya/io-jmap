//! RFC 9610 helpers: capability URN and account capability object.

use serde::Deserialize;

/// JMAP for Contacts capability (RFC 9610 §1.4.1).
pub const JMAP_CONTACTS_CAPABILITY: &str = "urn:ietf:params:jmap:contacts";

/// Value of the contacts capability in an account's `accountCapabilities`
/// property (RFC 9610 §1.4.1).
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapContactsCapability {
    /// Max number of AddressBooks assignable to a single ContactCard;
    /// `None` for no limit.
    #[serde(default)]
    pub max_address_books_per_card: Option<u64>,
    /// Whether the user may create an AddressBook in this account.
    #[serde(default)]
    pub may_create_address_book: bool,
}
