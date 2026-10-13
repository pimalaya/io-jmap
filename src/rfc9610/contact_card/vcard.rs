//! # vCard conversion
//!
//! Converts a ContactCard's JSContact Card (RFC 9553) to and from a vCard
//! following the RFC 9555 mapping, through vcard-rs.
//!
//! The conversion is lossless both ways, the members and properties with no
//! counterpart riding the vCardProps and JSPROP escape hatches. FN maps from
//! `name.full` alone, never composed from the name components.

use alloc::{
    collections::BTreeMap,
    string::{String, ToString},
};

use serde_json::{Map, Value};
use thiserror::Error;
use vcard::{
    tree::{cst::VcardCst, error::VcardParseError},
    vcard::Vcard,
};

use crate::rfc9610::contact_card::{JmapContactCard, set::JmapContactCardPatch};

impl JmapContactCard {
    /// Converts the card's JSContact properties to a vCard 4.0 document.
    ///
    /// The JMAP `id` and `addressBookIds` are not part of the card, and stay
    /// out of the document.
    pub fn to_vcard(&self) -> String {
        let card = Value::Object(self.card.clone());

        match Vcard::from_jscontact(&card) {
            Ok(vcard) => vcard.to_string(),
            Err(_) => unreachable!("an object root always converts"),
        }
    }

    /// Converts a vCard to a card with no id and no address book, the shape
    /// a `ContactCard/set` create takes once given its `addressBookIds`.
    pub fn from_vcard(vcard: &str) -> Result<Self, JmapContactCardVcardError> {
        let cst = VcardCst::parse(vcard).map_err(JmapContactCardVcardError::Parse)?;

        match cst.decode().to_jscontact() {
            Value::Object(card) => Ok(Self {
                card,
                ..Default::default()
            }),
            _ => Err(JmapContactCardVcardError::NotCard),
        }
    }

    /// Converts an edited vCard to the `ContactCard/set` update patch against
    /// its base, the vCard last synced (RFC 8620 §5.3).
    ///
    /// Each property differing goes whole, a removed one as a null, except the
    /// `uid` RFC 9610 §3 groups cards on. Without a base, every property goes,
    /// which cannot clear the server-side ones the edited vCard lost track of.
    pub fn update_from_vcard(
        vcard: &str,
        base_vcard: Option<&str>,
    ) -> Result<JmapContactCardPatch, JmapContactCardVcardError> {
        let card = Self::from_vcard(vcard)?.card;
        let mut patch = match base_vcard {
            Some(base) => diff(card, &Self::from_vcard(base)?.card),
            None => card.into_iter().collect(),
        };

        // NOTE: the JMAP envelope is not part of the JSContact payload, and
        // addressBookIds in particular must survive the update.
        patch.remove("id");
        patch.remove("addressBookIds");

        Ok(JmapContactCardPatch(patch))
    }
}

/// A vCard that cannot become a ContactCard.
#[derive(Debug, Error)]
pub enum JmapContactCardVcardError {
    /// The vCard does not parse.
    #[error("Invalid vCard: {0}")]
    Parse(#[source] VcardParseError),
    /// The conversion produced no JSContact Card object.
    #[error("JSContact conversion did not produce a card object")]
    NotCard,
}

/// The top-level members of `card` differing from `base`, and a null for
/// each one `base` holds that `card` lost, `uid` aside.
fn diff(card: Map<String, Value>, base: &Map<String, Value>) -> BTreeMap<String, Value> {
    let mut patch = BTreeMap::new();

    for key in base.keys() {
        if !card.contains_key(key) && key != "uid" {
            patch.insert(key.clone(), Value::Null);
        }
    }

    for (key, value) in card {
        if base.get(&key) != Some(&value) {
            patch.insert(key, value);
        }
    }

    patch
}

#[cfg(test)]
mod tests {
    use alloc::string::ToString;

    use serde_json::{Value, json};

    use crate::rfc9610::contact_card::JmapContactCard;

    const VCARD: &str = "BEGIN:VCARD\r\nVERSION:4.0\r\nUID:abc\r\nFN:Jane Doe\r\nEMAIL:jane@example.com\r\nTEL:+33612345678\r\nEND:VCARD\r\n";

    #[test]
    fn vcard_to_jscontact_maps_core_properties() {
        let card = JmapContactCard::from_vcard(VCARD).unwrap();

        assert_eq!(card.id, None);
        assert!(card.address_book_ids.is_empty());
        let card = card.card;
        assert_eq!(
            card.get("uid").and_then(|uid| uid.as_str()),
            Some("abc"),
            "{card:?}"
        );
        assert!(card.contains_key("name"), "{card:?}");
        assert!(card.contains_key("emails"), "{card:?}");
        assert!(card.contains_key("phones"), "{card:?}");
    }

    #[test]
    fn jscontact_to_vcard_round_trips() {
        let card = JmapContactCard::from_vcard(VCARD).unwrap();
        let vcard = card.to_vcard();

        assert!(vcard.contains("FN:Jane Doe"), "{vcard}");
        assert!(vcard.contains("UID:abc"), "{vcard}");
        assert!(vcard.contains("jane@example.com"), "{vcard}");
    }

    #[test]
    fn jmap_envelope_stays_out_of_the_vcard() {
        let card: JmapContactCard = serde_json::from_value(json!({
            "id": "k1",
            "addressBookIds": { "b1": true },
            "@type": "Card",
            "version": "1.0",
            "uid": "urn:uuid:1",
            "name": { "full": "Jane 1" },
        }))
        .unwrap();

        assert_eq!(
            card.to_vcard(),
            "BEGIN:VCARD\r\nVERSION:4.0\r\nFN:Jane 1\r\nUID:urn:uuid:1\r\nEND:VCARD\r\n"
        );
    }

    #[test]
    fn invalid_vcard_is_refused() {
        let err = JmapContactCard::from_vcard("not a vcard").unwrap_err();
        assert!(err.to_string().starts_with("Invalid vCard: "), "{err}");

        let err = JmapContactCard::update_from_vcard(VCARD, Some("")).unwrap_err();
        assert!(err.to_string().starts_with("Invalid vCard: "), "{err}");
    }

    #[test]
    fn patch_without_base_carries_every_property() {
        let patch = JmapContactCard::update_from_vcard(VCARD, None).unwrap().0;

        assert!(patch.contains_key("name"), "{patch:?}");
        assert!(patch.contains_key("emails"), "{patch:?}");
        assert!(!patch.contains_key("id"), "{patch:?}");
        assert!(!patch.contains_key("addressBookIds"), "{patch:?}");
    }

    #[test]
    fn patch_against_base_keeps_only_changes() {
        let edited = VCARD.replace("Jane Doe", "Jane Smith");
        let patch = JmapContactCard::update_from_vcard(&edited, Some(VCARD))
            .unwrap()
            .0;

        assert!(patch.contains_key("name"), "{patch:?}");
        assert!(!patch.contains_key("emails"), "{patch:?}");
        assert!(!patch.contains_key("phones"), "{patch:?}");
        assert!(!patch.contains_key("uid"), "{patch:?}");
    }

    #[test]
    fn patch_nulls_removed_properties() {
        let edited = VCARD.replace("TEL:+33612345678\r\n", "");
        let patch = JmapContactCard::update_from_vcard(&edited, Some(VCARD))
            .unwrap()
            .0;

        assert_eq!(patch.get("phones"), Some(&Value::Null));
        assert!(!patch.contains_key("uid"), "{patch:?}");
    }

    #[test]
    fn patch_never_nulls_the_uid() {
        let edited = VCARD.replace("UID:abc\r\n", "");
        let patch = JmapContactCard::update_from_vcard(&edited, Some(VCARD))
            .unwrap()
            .0;

        assert!(patch.is_empty(), "{patch:?}");
    }

    #[test]
    fn middle_name_rides_given2_and_round_trips() {
        // NOTE: RFC 9553 carries the middle name as the given2 component
        // kind; folding it into given (or dropping it) would destroy N's
        // third component through every JMAP round-trip.
        let vcard = "BEGIN:VCARD\r\nVERSION:4.0\r\nUID:abc\r\nN:BB;Aa;g;;\r\nEND:VCARD\r\n";
        let card = JmapContactCard::from_vcard(vcard).unwrap();

        let components = card
            .card
            .get("name")
            .and_then(|name| name.get("components"))
            .and_then(|components| components.as_array())
            .expect("name components");
        let given2 = components.iter().any(|component| {
            component.get("kind").and_then(|kind| kind.as_str()) == Some("given2")
                && component.get("value").and_then(|value| value.as_str()) == Some("g")
        });
        assert!(given2, "{card:?}");

        let round = card.to_vcard();
        assert!(round.contains("N:BB;Aa;g;;"), "{round}");
    }

    #[test]
    fn name_components_without_full_mint_no_display_name() {
        // NOTE: a card with name components but no name.full converts
        // without an FN, never minting a display name into the record.
        let card: JmapContactCard = serde_json::from_value(json!({
            "@type": "Card",
            "version": "1.0",
            "uid": "abc",
            "name": {
                "components": [
                    { "kind": "given", "value": "Jane" },
                    { "kind": "surname", "value": "Doe" },
                ],
            },
        }))
        .unwrap();

        let vcard = card.to_vcard();

        assert!(!vcard.contains("FN"), "{vcard}");
        assert!(vcard.contains("Jane"), "{vcard}");
    }
}
