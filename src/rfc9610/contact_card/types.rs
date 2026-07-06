//! JMAP ContactCard types (RFC 9610 §3).

use alloc::{collections::BTreeMap, string::String, vec::Vec};
use core::fmt;

use serde::{Deserialize, Serialize, Serializer};

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

/// Patch object for `ContactCard/set` update requests (RFC 8620 §5.3): JSON
/// pointer paths mapped to their new values; a `null` value removes the
/// pointed property.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(transparent)]
pub struct JmapContactCardPatch(pub BTreeMap<String, serde_json::Value>);

/// Arguments for copying a single card between accounts via
/// `ContactCard/copy` (RFC 9610 §3.6).
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapContactCardCopyArgs {
    /// Source ContactCard id.
    pub id: String,
    /// `{ address-book-id -> true }` in the destination account.
    pub address_book_ids: BTreeMap<String, bool>,
}

/// Filter for `ContactCard/query` (RFC 9610 §3.3.1); all specified
/// conditions must apply.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapContactCardFilter {
    /// AddressBook id the card must be in.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub in_address_book: Option<String>,

    /// Exact JSContact `uid` of the card.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uid: Option<String>,

    /// Uid the card's `members` property must contain.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_member: Option<String>,

    /// Exact JSContact `kind` of the card, e.g. `group`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,

    /// The card's `created` date-time must be before this UTC date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_before: Option<String>,

    /// The card's `created` date-time must be the same or after this UTC
    /// date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_after: Option<String>,

    /// The card's `updated` date-time must be before this UTC date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_before: Option<String>,

    /// The card's `updated` date-time must be the same or after this UTC
    /// date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_after: Option<String>,

    /// Free-text match against any text in the card.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,

    /// Match against any NameComponent or the full name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// Match against NameComponents of kind `given`.
    #[serde(rename = "name/given", skip_serializing_if = "Option::is_none")]
    pub name_given: Option<String>,

    /// Match against NameComponents of kind `surname`.
    #[serde(rename = "name/surname", skip_serializing_if = "Option::is_none")]
    pub name_surname: Option<String>,

    /// Match against NameComponents of kind `surname2`.
    #[serde(rename = "name/surname2", skip_serializing_if = "Option::is_none")]
    pub name_surname2: Option<String>,

    /// Match against any Nickname name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nickname: Option<String>,

    /// Match against any Organization name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub organization: Option<String>,

    /// Match against any EmailAddress address or label.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,

    /// Match against any Phone number or label.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phone: Option<String>,

    /// Match against any OnlineService service, uri, user or label.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub online_service: Option<String>,

    /// Match against any AddressComponent or the full address.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,

    /// Match against any Note note.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// Sort property for `ContactCard/query` (RFC 9610 §3.3.2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JmapContactCardSortProperty {
    /// The `created` date on the ContactCard.
    Created,
    /// The `updated` date on the ContactCard.
    Updated,
    /// The first NameComponent of kind `given`.
    NameGiven,
    /// The first NameComponent of kind `surname`.
    NameSurname,
    /// The first NameComponent of kind `surname2`.
    NameSurname2,
}

impl fmt::Display for JmapContactCardSortProperty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Created => "created",
            Self::Updated => "updated",
            Self::NameGiven => "name/given",
            Self::NameSurname => "name/surname",
            Self::NameSurname2 => "name/surname2",
        })
    }
}

impl Serialize for JmapContactCardSortProperty {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

/// Sort comparator for `ContactCard/query` (RFC 8620 §5.5).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapContactCardSortComparator {
    pub property: JmapContactCardSortProperty,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_ascending: Option<bool>,
}

/// Per-object error returned in `ContactCard/set` responses (RFC 9610 §3.5).
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum JmapContactCardSetItemError {
    /// One or more blob IDs in the card (e.g. a Media `blobId`) were not
    /// found (RFC 9610 §3).
    BlobNotFound { description: Option<String> },
    /// Standard set error (RFC 8620 §5.3): the change is not allowed.
    Forbidden { description: Option<String> },
    /// Standard set error (RFC 8620 §5.3): target id not found.
    NotFound { description: Option<String> },
    /// Standard set error (RFC 8620 §5.3): patch could not be applied.
    InvalidPatch { description: Option<String> },
    /// Standard set error (RFC 8620 §5.3): would destroy an object already
    /// queued for destruction in the same request.
    WillDestroy { description: Option<String> },
    /// Standard set error (RFC 8620 §5.3): one or more properties were
    /// invalid.
    InvalidProperties {
        description: Option<String>,
        #[serde(default)]
        properties: Vec<String>,
    },
    /// Catch-all for set errors not modelled above.
    #[serde(other)]
    Unknown,
}

/// Per-object error returned in `ContactCard/copy` responses (RFC 8620
/// §5.4).
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum JmapContactCardCopyItemError {
    /// The card already exists in the destination account (RFC 8620 §5.4).
    AlreadyExists { description: Option<String> },
    /// Standard set error (RFC 8620 §5.3): target id not found.
    NotFound { description: Option<String> },
    /// Standard set error (RFC 8620 §5.3): one or more properties were
    /// invalid.
    InvalidProperties {
        description: Option<String>,
        #[serde(default)]
        properties: Vec<String>,
    },
    /// Catch-all for set errors not modelled above.
    #[serde(other)]
    Unknown,
}
