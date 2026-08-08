//! JMAP for Calendars: Calendar (draft-ietf-jmap-calendars-27).

use alloc::{collections::BTreeMap, string::String};

use serde::{Deserialize, Serialize};

pub mod changes;
pub mod get;

/// A JMAP Calendar object (draft-ietf-jmap-calendars): a named
/// collection of CalendarEvents.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapCalendar {
    /// The server-assigned calendar id.
    pub id: Option<String>,
    /// The user-visible calendar name.
    pub name: Option<String>,
    /// Optional long-form description providing context in shared
    /// environments.
    pub description: Option<String>,
    /// Colour to display the calendar's events with, as a CSS colour.
    pub color: Option<String>,
    /// Position hint for display ordering (lower first).
    #[serde(default)]
    pub sort_order: u32,
    /// True for at most one Calendar per account (server-set).
    #[serde(default)]
    pub is_default: bool,
    /// Whether the user is subscribed to the calendar.
    #[serde(default)]
    pub is_subscribed: bool,
    /// Whether the calendar's events are displayed by default; the
    /// draft defaults it to true, so an absent value means the server
    /// was not asked for the property rather than a hidden calendar.
    #[serde(default)]
    pub is_visible: Option<bool>,
    /// Which of the calendar's events count towards the user's
    /// availability.
    #[serde(default)]
    pub include_in_availability: JmapCalendarAvailability,
    /// Alerts applied to events with a time zone that opt in through
    /// their `useDefaultAlerts` property, as raw JSCalendar Alert
    /// objects keyed by alert id.
    #[serde(default)]
    pub default_alerts_with_time: Option<serde_json::Map<String, serde_json::Value>>,
    /// Same as [`JmapCalendar::default_alerts_with_time`] for all-day
    /// events, which have no time zone to anchor an alert to.
    #[serde(default)]
    pub default_alerts_without_time: Option<serde_json::Map<String, serde_json::Value>>,
    /// IANA time zone id the calendar's floating times are resolved
    /// against; `None` falls back to the account default.
    pub time_zone: Option<String>,
    /// Principal id to rights map (RFC 9670); `None` when unshared or
    /// when the server does not support sharing.
    #[serde(default)]
    pub share_with: Option<BTreeMap<String, JmapCalendarRights>>,
    /// The user's rights on the calendar.
    #[serde(default)]
    pub my_rights: JmapCalendarRights,
}

/// How a Calendar's events contribute to the user's free/busy
/// availability (draft-ietf-jmap-calendars).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum JmapCalendarAvailability {
    /// Every event in the calendar counts as busy.
    #[default]
    All,
    /// Only events the user is a confirmed or tentative participant of
    /// count as busy.
    Attending,
    /// The calendar never contributes to availability.
    None,
}

/// Access rights on a Calendar (draft-ietf-jmap-calendars).
///
/// Every right defaults to false when the server omits it, the set
/// being still under revision: a renamed right must not fail the whole
/// fetch.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct JmapCalendarRights {
    /// May read the busy periods of the calendar's events without
    /// seeing the events themselves.
    pub may_read_free_busy: bool,
    /// May fetch the CalendarEvents in this Calendar.
    pub may_read_items: bool,
    /// May create, modify or destroy any CalendarEvent in this
    /// Calendar, or move one to or from it.
    pub may_write_all: bool,
    /// May do the same as [`JmapCalendarRights::may_write_all`], but
    /// only for events the user owns.
    pub may_write_own: bool,
    /// May modify the per-user properties (alerts, keywords, colour) of
    /// events the user does not own.
    pub may_update_private: bool,
    /// May modify the user's own participation status on events they
    /// are invited to.
    #[serde(rename = "mayRSVP")]
    pub may_rsvp: bool,
    /// May modify the `shareWith` property of this Calendar.
    pub may_share: bool,
    /// May delete the Calendar itself.
    pub may_delete: bool,
}
