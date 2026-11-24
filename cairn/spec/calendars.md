---
cairn: spec
capability: calendars
status: current
---

# Calendars

JMAP for Calendars (draft-ietf-jmap-calendars-27), the one domain of the JMAP suite still moving through the working group. It lives under `src/calendars/`, named for its domain instead of its RFC, and is renamed to `rfcNNNN` the day the number exists; the type names are already stable.

### Requirement: The draft revision is named once
The module header of `src/calendars.rs` SHALL name the draft revision the module implements, and no other place in the module SHALL name a revision or cite a section number, a reference into a moving target being worse than none.

### Requirement: Calendars are read and written
io-jmap SHALL cover `Calendar/get` (`JmapCalendarGet`, its properties a closed `JmapCalendarProperty` enum), `Calendar/changes` (`JmapCalendarChanges`) and `Calendar/set` (`JmapCalendarSet`). `Calendar/set` SHALL create, update by patch (`JmapCalendarUpdate`, a nullable property cleared by `Some(None)`) and destroy calendars, sending the draft's `onDestroyRemoveEvents` and `onSuccessSetIsDefault`, and SHALL type its per-object errors as `JmapCalendarSetItemError`, the RFC 8620 §5.3 ones plus the draft's `calendarHasEvent`.

`JmapCalendarRights` SHALL read a right the server omits as false rather than fail the whole `Calendar/get`, and SHALL read `mayRSVP` under that spelling. `isVisible` SHALL read as `Option<bool>`, `None` meaning the server was not asked, since the draft defaults it to true.

#### Scenario: A right renamed between revisions
- GIVEN a `Calendar/get` response whose rights object lacks one right
- WHEN the response is parsed
- THEN the calendar parses, the missing right false

### Requirement: Events are read
io-jmap SHALL cover `CalendarEvent/get` (`JmapCalendarEventGet`), carrying the draft's `recurrenceOverridesBefore`, `recurrenceOverridesAfter`, `reduceParticipants` and `timeZone` arguments, `CalendarEvent/query` (`JmapCalendarEventQuery`) with its own time-range filter and sort, recurrence expanded by the server, and `CalendarEvent/changes` (`JmapCalendarEventChanges`). `CalendarEvent/get` SHALL build its own method call and decode through the generic `JmapGet`.

### Requirement: The JSCalendar payload stays raw JSON
`JmapCalendarEvent` SHALL keep its JSCalendar `Event` or `Task` (RFC 8984) as a `serde_json::Map` flattened into the object, the way `JmapContactCard` keeps its JSContact payload, and its properties SHALL be strings, the JSCalendar namespace being open. Only the JMAP properties (id, `calendarIds`, draft and origin flags, `utcStart` / `utcEnd`) are typed.

### Requirement: Events are written through CalendarEvent/set
`JmapCalendarEventSet` SHALL create, update by patch and destroy events, honour `ifInState`, and send `sendSchedulingMessages`, decoding through the generic `JmapSet`. Its per-object errors SHALL be typed as `JmapCalendarEventSetItemError`, the RFC 8620 §5.3 ones plus the draft's `noSupportedScheduleMethods`. A create SHALL NOT carry the server-set `isOrigin`.

`JmapCalendarEventPatch::diff` SHALL compute the patch from a base event to a staged one: each differing top-level member whole, a removed one as null, `uid` never nulled, and `recurrenceOverrides` patched per recurrence id.

### Requirement: iCalendar converts into an event behind the ical feature
With the `ical` feature, `JmapCalendarEvent::from_ical` SHALL convert a parsed iCalendar object into the event of its one JSCalendar 2.0 entry, with no id and no calendar: overrides fold into `recurrenceOverrides`, an `EXDATE` into an `excluded` override, and the entry's `method` is dropped. The event SHALL carry the Group's `version` (`2.0`), a stored event being a top-level JSCalendar object: Fastmail refuses an event without it (`invalidProperties`, `version`), Stalwart 0.16 accepts both. `JmapCalendarEvent::to_ical` SHALL convert the JSCalendar payload back to an iCalendar object, the JMAP properties left out. Without the feature io-jmap SHALL NOT depend on ical-rs.

### Requirement: The rest of the draft is out
`CalendarEvent/copy`, `CalendarEvent/parse`, `CalendarEventNotification/*` and `ParticipantIdentity/*` SHALL stay out until a consumer writes or reads them, a coroutine with no caller being untested surface.
