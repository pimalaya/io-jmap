//! JMAP for Calendars: CalendarEvent (draft-ietf-jmap-calendars-27).

use alloc::{collections::BTreeMap, string::String};

use serde::{Deserialize, Serialize};

pub mod changes;
pub mod get;
pub mod query;

/// A JMAP CalendarEvent object (draft-ietf-jmap-calendars): a JSCalendar
/// Event or Task (RFC 8984) extended with the JMAP properties below.
///
/// The JSCalendar payload is kept as raw JSON in
/// [`JmapCalendarEvent::event`]; modelling or converting it (e.g. to
/// iCalendar per RFC 5545) is out of scope for this crate. The
/// draft-defined JSCalendar extensions stay in there too, `mayInviteSelf`
/// and `hideAttendees` being event content rather than JMAP plumbing.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapCalendarEvent {
    /// The id of the CalendarEvent (immutable; server-set); may differ
    /// from the JSCalendar `uid`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// The id of the event this one is an alternative instance of, for
    /// the events a server splits out of a recurring series; `None` for
    /// a standalone event or the series itself.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_event_id: Option<String>,
    /// The set of Calendar ids this event belongs to; an event belongs
    /// to at least one Calendar at all times.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub calendar_ids: BTreeMap<String, bool>,
    /// Whether the event is a draft the user has not finished editing;
    /// only ever true before the first non-draft update.
    #[serde(default)]
    pub is_draft: bool,
    /// Whether this account is the origin of the event, as opposed to a
    /// copy received through scheduling (server-set).
    #[serde(default)]
    pub is_origin: bool,
    /// The event's start in UTC, computed by the server so a client can
    /// sort without expanding recurrence itself; fetch-time only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub utc_start: Option<String>,
    /// The event's end in UTC, computed the same way as
    /// [`JmapCalendarEvent::utc_start`]; fetch-time only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub utc_end: Option<String>,
    /// The JSCalendar Event or Task properties (RFC 8984), kept as raw
    /// JSON.
    #[serde(flatten)]
    pub event: serde_json::Map<String, serde_json::Value>,
}
