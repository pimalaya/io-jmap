---
cairn: log
change: an-event-carries-its-version
landed: 2026-11-24
---

# An event carries its version

Landed without a proposal (a one-line fix asked by calendula's live night, cairn/changes/the-live-night-findings there, task C1b).

`JmapCalendarEvent::from_ical` (src/calendars/calendar_event/ical.rs) lifted the one entry out of ical-rs's JSCalendar 2.0 Group and dropped the Group with its `version`. A stored JMAP event is a top-level JSCalendar object, and Fastmail refuses a `CalendarEvent/set` create without `version: "2.0"` (`invalidProperties`, `version`); Stalwart 0.16 accepted it, so the night's Stalwart runs passed. The entry now takes the Group's `version`. `JmapCalendarEventPatch::diff` is unchanged: both sides of an update carry the same version, so no patch names it.

Capabilities moved: calendars ("iCalendar converts into an event behind the ical feature" modified).

Tests: `a_series_is_written_as_jscalendar_2_0_without_its_method` asserted `version` absent, encoding the bug; it is now `a_series_is_written_as_a_jscalendar_2_0_object_without_its_method` and asserts `version` is `"2.0"`, failing on the old code (`left: None`, `right: Some(String("2.0"))`). Live: calendula's event create, update and delete on Fastmail JMAP through a scratch build patched onto this tree (calendula's tasks.md, C1b).
