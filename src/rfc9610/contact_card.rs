//! JMAP for Contacts: ContactCard (RFC 9610 §3).

use alloc::{collections::BTreeMap, string::String};

use serde::{Deserialize, Serialize};

pub mod changes;
pub mod copy;
pub mod get;
pub mod query;
pub mod set;

/// A JMAP ContactCard object (RFC 9610 §3): a JSContact Card (RFC 9553 §2)
/// extended with the JMAP `id` and `addressBookIds` properties.
///
/// The JSContact payload is kept as raw JSON in [`JmapContactCard::card`];
/// modelling or converting it (e.g. to vCard per RFC 9555) is out of scope
/// for this crate.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapContactCard {
    /// The id of the ContactCard (immutable; server-set); may differ from
    /// the JSContact `uid`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// The set of AddressBook ids this card belongs to; a card belongs to
    /// at least one AddressBook at all times.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub address_book_ids: BTreeMap<String, bool>,
    /// The JSContact Card properties (RFC 9553 §2), kept as raw JSON.
    #[serde(flatten)]
    pub card: serde_json::Map<String, serde_json::Value>,
}
