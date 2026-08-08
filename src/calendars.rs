//! JMAP for Calendars (draft-ietf-jmap-calendars-27).
//!
//! The only domain of the JMAP suite still moving through the working
//! group, hence the only module named for its domain instead of its
//! RFC: it is renamed to `rfcNNNN` the day the number exists, the type
//! names being already stable. Revisions are named here and nowhere
//! else, and no section number is cited anywhere in the module, a
//! reference into a moving target being worse than none.
//!
//! Two data types are covered, both read-only for now: [`calendar`],
//! the collection, and [`calendar_event`], a JSCalendar Event or Task
//! (RFC 8984) kept as raw JSON the way [`crate::rfc9610`] keeps its
//! JSContact payload.

use alloc::string::String;

use serde::Deserialize;

pub mod calendar;
pub mod calendar_event;

/// JMAP for Calendars capability (draft-ietf-jmap-calendars).
pub const JMAP_CALENDARS_CAPABILITY: &str = "urn:ietf:params:jmap:calendars";

/// Value of the calendars capability in an account's
/// `accountCapabilities` property (draft-ietf-jmap-calendars).
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapCalendarsCapability {
    /// Max number of Calendars assignable to a single CalendarEvent;
    /// `None` for no limit.
    #[serde(default)]
    pub max_calendars_per_event: Option<u64>,
    /// Earliest date-time the server accepts, in UTC.
    #[serde(default)]
    pub min_date_time: Option<String>,
    /// Latest date-time the server accepts, in UTC.
    #[serde(default)]
    pub max_date_time: Option<String>,
    /// Longest time range an expanded `CalendarEvent/query` may span,
    /// as an ISO 8601 duration.
    #[serde(default)]
    pub max_expanded_query_duration: Option<String>,
    /// Max number of participants a single CalendarEvent may carry;
    /// `None` for no limit.
    #[serde(default)]
    pub max_participants_per_event: Option<u64>,
    /// Whether the user may create a Calendar in this account.
    #[serde(default)]
    pub may_create_calendar: bool,
}
