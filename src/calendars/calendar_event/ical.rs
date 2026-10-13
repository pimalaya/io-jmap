//! # iCalendar conversion
//!
//! Converts a decoded iCalendar object (RFC 5545) to the CalendarEvent of
//! draft-ietf-jmap-calendars, through the ical-rs JSCalendar mapping.
//!
//! The draft builds on JSCalendar 2.0 (draft-ietf-calext-jscalendarbis), so
//! the object is converted at that version. The update patch between two
//! converted events is [`JmapCalendarEventPatch::diff`].
//!
//! [`JmapCalendarEventPatch::diff`]: crate::calendars::calendar_event::set::JmapCalendarEventPatch::diff

use ical::{
    component::IcalComponent, ical::Ical, jscalendar::IcalJscalendarVersion, prop::IcalPropKind,
};
use serde_json::Value;
use thiserror::Error;

use crate::calendars::calendar_event::JmapCalendarEvent;

impl JmapCalendarEvent {
    /// Converts a calendar object to the event of its one JSCalendar 2.0
    /// entry, with no id and no calendar.
    ///
    /// Overrides fold into `recurrenceOverrides`, an `EXDATE` an `excluded`
    /// one, and the entry's `method` is dropped, a stored event carrying none.
    /// The caller parses the object, bounding its nesting depth when untrusted.
    pub fn from_ical(calendar: &Ical<'_>) -> Result<Self, JmapCalendarEventIcalError> {
        let rules = |component: &IcalComponent| {
            component
                .props
                .iter()
                .filter(|prop| prop.name == IcalPropKind::RRule.into())
                .count()
        };
        if calendar
            .components
            .iter()
            .any(|component| rules(component) > 1)
        {
            return Err(JmapCalendarEventIcalError::RecurrenceRules);
        }

        let mut entries = match calendar.to_jscalendar_as(IcalJscalendarVersion::V2_0) {
            Value::Object(mut group) => group.remove("entries"),
            _ => None,
        };
        let entry = match entries.as_mut() {
            Some(Value::Array(entries)) if entries.len() == 1 => entries.pop(),
            _ => None,
        };
        let Some(Value::Object(mut event)) = entry else {
            return Err(JmapCalendarEventIcalError::NotOneEvent);
        };

        // NOTE: 2.0 states the calendar's `METHOD` on each entry, but the
        // draft has a stored event carry none, a server refusing one as
        // invalid; it belongs to a scheduling message.
        event.remove("method");

        Ok(Self {
            event,
            ..Default::default()
        })
    }
}

/// A calendar object that cannot become a CalendarEvent.
#[derive(Debug, Error)]
pub enum JmapCalendarEventIcalError {
    /// A component holds several `RRULE`s, JSCalendar 2.0 holding one.
    ///
    /// A second rule would ride the escape hatch, which Stalwart 0.16 drops.
    #[error("A JMAP event holds one recurrence rule, not several")]
    RecurrenceRules,
    /// The object converts to no entry, or to several.
    #[error("The calendar object is not one JMAP event")]
    NotOneEvent,
}

#[cfg(test)]
mod tests {
    use alloc::{
        string::{String, ToString},
        vec::Vec,
    };

    use ical::tree::cst::IcalCst;
    use serde_json::{Map, Value};

    use crate::calendars::calendar_event::{
        JmapCalendarEvent, ical::JmapCalendarEventIcalError, set::JmapCalendarEventPatch,
    };

    const NEW_ENTRY: &str = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Pimalaya//Test//EN\r\n\
        BEGIN:VEVENT\r\nUID:lunch\r\nDTSTAMP:20261009T080000Z\r\n\
        DTSTART;TZID=Europe/Paris:20261012T120000\r\nDURATION:PT1H\r\nSUMMARY:Lunch\r\n\
        END:VEVENT\r\nEND:VCALENDAR\r\n";

    /// A weekly standup series, with one occurrence moved.
    const SERIES: &str = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Pimalaya//Test//EN\r\n\
        BEGIN:VEVENT\r\nUID:standup\r\nDTSTAMP:20261009T080000Z\r\n\
        DTSTART;TZID=Europe/Paris:20261001T090000\r\nDURATION:PT30M\r\n\
        SUMMARY:Standup\r\nRRULE:FREQ=WEEKLY\r\nEND:VEVENT\r\n\
        BEGIN:VEVENT\r\nUID:standup\r\nDTSTAMP:20261009T080000Z\r\n\
        RECURRENCE-ID;TZID=Europe/Paris:20261008T090000\r\n\
        DTSTART;TZID=Europe/Paris:20261008T100000\r\nDURATION:PT30M\r\n\
        SUMMARY:Standup, later\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";

    fn event(ical: &str) -> Result<Map<String, Value>, JmapCalendarEventIcalError> {
        let cst = IcalCst::parse(ical).unwrap();
        JmapCalendarEvent::from_ical(&cst.decode()).map(|event| event.event)
    }

    fn keys(patch: &JmapCalendarEventPatch) -> Vec<&str> {
        patch.0.keys().map(String::as_str).collect()
    }

    #[test]
    fn one_event_converts_to_its_entry() {
        let cst = IcalCst::parse(NEW_ENTRY).unwrap();
        let event = JmapCalendarEvent::from_ical(&cst.decode()).unwrap();

        assert_eq!(event.id, None);
        assert!(event.calendar_ids.is_empty());
        assert_eq!(event.event["@type"], "Event");
        assert_eq!(event.event["uid"], "lunch");
        assert_eq!(event.event["title"], "Lunch");
        assert_eq!(event.event["start"], "2026-10-12T12:00:00");
        assert_eq!(event.event["timeZone"], "Europe/Paris");
    }

    #[test]
    fn a_series_is_written_as_jscalendar_2_0_without_its_method() {
        let series = NEW_ENTRY
            .replace("VERSION:2.0\r\n", "VERSION:2.0\r\nMETHOD:REQUEST\r\n")
            .replace(
                "SUMMARY:Lunch\r\n",
                "SUMMARY:Lunch\r\nRRULE:FREQ=WEEKLY;COUNT=4\r\n\
                 ORGANIZER:mailto:jane@example.com\r\nATTENDEE;CN=Ada:mailto:ada@example.com\r\n",
            );

        let event = event(&series).unwrap();

        assert!(event.get("method").is_none(), "{event:?}");
        assert!(event.get("version").is_none(), "{event:?}");
        assert_eq!(event["recurrenceRule"]["frequency"], "weekly");
        assert_eq!(event["recurrenceRule"]["count"], 4);
        assert!(event.get("recurrenceRules").is_none(), "{event:?}");
        assert_eq!(event["organizerCalendarAddress"], "mailto:jane@example.com");
        let ada = event["participants"]
            .as_object()
            .unwrap()
            .values()
            .find(|participant| participant["name"] == "Ada")
            .expect("the attendee");
        assert_eq!(ada["calendarAddress"], "mailto:ada@example.com");
    }

    #[test]
    fn an_override_folds_into_recurrence_overrides() {
        let event = event(SERIES).unwrap();

        let over = &event["recurrenceOverrides"]["2026-10-08T09:00:00"];
        assert_eq!(over["start"], "2026-10-08T10:00:00", "{over}");
        assert_eq!(over["title"], "Standup, later", "{over}");
    }

    #[test]
    fn an_object_of_two_events_is_refused() {
        let two = NEW_ENTRY.replace(
            "END:VEVENT\r\n",
            "END:VEVENT\r\nBEGIN:VEVENT\r\nUID:other\r\nDTSTAMP:20261009T080000Z\r\n\
             DTSTART:20261013T120000\r\nEND:VEVENT\r\n",
        );
        let none = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:x\r\nEND:VCALENDAR\r\n";

        for ical in [two.as_str(), none] {
            let err = event(ical).unwrap_err();
            assert!(
                matches!(err, JmapCalendarEventIcalError::NotOneEvent),
                "{err}"
            );
            assert_eq!(err.to_string(), "The calendar object is not one JMAP event");
        }
    }

    #[test]
    fn a_series_of_two_rules_is_refused() {
        let rules = "SUMMARY:Lunch\r\nRRULE:FREQ=WEEKLY\r\nRRULE:FREQ=MONTHLY\r\n";
        let err = event(&NEW_ENTRY.replace("SUMMARY:Lunch\r\n", rules)).unwrap_err();

        assert!(
            matches!(err, JmapCalendarEventIcalError::RecurrenceRules),
            "{err}"
        );
        assert!(err.to_string().contains("one recurrence rule"), "{err}");
    }

    #[test]
    fn an_edit_patches_only_what_it_changed() {
        let base = event(SERIES).unwrap();
        let staged = event(&SERIES.replace("SUMMARY:Standup\r\n", "SUMMARY:Daily\r\n")).unwrap();

        let patch = JmapCalendarEventPatch::diff(&staged, &base);

        assert_eq!(keys(&patch), ["title"]);
        assert_eq!(patch.0["title"], "Daily");
        assert!(JmapCalendarEventPatch::diff(&base, &base).0.is_empty());
    }

    #[test]
    fn an_occurrence_moved_again_patches_its_override_alone() {
        let base = event(SERIES).unwrap();
        let staged = event(&SERIES.replace(
            "DTSTART;TZID=Europe/Paris:20261008T100000",
            "DTSTART;TZID=Europe/Paris:20261008T110000",
        ))
        .unwrap();

        let patch = JmapCalendarEventPatch::diff(&staged, &base);

        assert_eq!(keys(&patch), ["recurrenceOverrides/2026-10-08T09:00:00"]);
        let over = &patch.0["recurrenceOverrides/2026-10-08T09:00:00"];
        assert_eq!(over["start"], "2026-10-08T11:00:00", "{over}");
    }
}
