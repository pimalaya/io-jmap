---
cairn: log
change: calendars-are-written
landed: 2026-11-18
---

# Calendars are written

Landed without a proposal (a coroutine mirroring `AddressBook/set`, asked by calendula's JMAP backend, cairn/changes/jmap-backend there).

- `JmapCalendarSet` (src/calendars/calendar/set.rs): `Calendar/set` create, patch update and destroy through the generic `JmapSet`, with the draft's `onDestroyRemoveEvents` and `onSuccessSetIsDefault`; `JmapCalendarSetItemError` types `calendarHasEvent` beside the RFC 8620 §5.3 errors and keeps an unmodelled one as `Other`. `JmapClientStd::calendar_set` runs it.
- `JmapCalendarEvent::to_ical` (behind `ical`): the event's JSCalendar payload back to an iCalendar object through ical-rs `Ical::from_jscalendar`, the JMAP properties left out, as `JmapContactCard::to_vcard` does for cards.

The spec's "The rest of the draft is out" no longer lists `Calendar/set`, its consumer now existing.

Tests: `update_clears_a_nullable_property`, `a_refusal_reads_as_its_type_and_description`, `an_unmodelled_error_keeps_its_type_and_description` (set.rs) and `an_event_converts_back_to_its_calendar_object` (ical.rs). Not run against a server yet: calendula's manual test is the first.

Capabilities moved: calendars, responses.
