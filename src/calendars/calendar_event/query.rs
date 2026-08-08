//! Batched JMAP `CalendarEvent/query` + `CalendarEvent/get` coroutine
//! (draft-ietf-jmap-calendars-27): single HTTP request, server-side
//! `#ids` back-reference resolves the get against the query results.
//!
//! Asking the server to expand recurrences is what makes this cheaper
//! than the CalDAV equivalent: one occurrence per id over the queried
//! window, no local expander.
//!
//! # Example
//!
//! ```rust,no_run
//! use std::{
//!     io::{Read, Write},
//!     net::TcpStream,
//! };
//!
//! use io_jmap::{
//!     calendars::calendar_event::query::{
//!         JmapCalendarEventFilter, JmapCalendarEventQuery, JmapCalendarEventQueryOptions,
//!     },
//!     coroutine::{JmapCoroutine, JmapCoroutineState, JmapYield},
//!     rfc8620::session::JmapSession,
//! };
//! use secrecy::SecretString;
//!
//! // Ready stream needed (TCP-connected, TLS-negociated)
//! let mut stream = TcpStream::connect("api.example.com:443").unwrap();
//! let mut buf = [0u8; 4096];
//!
//! let session: JmapSession = serde_json::from_str(r#"{
//!     "username": "",
//!     "accounts": {},
//!     "primaryAccounts": {"urn:ietf:params:jmap:calendars": "a1"},
//!     "capabilities": {},
//!     "apiUrl": "https://api.example.com/jmap/",
//!     "downloadUrl": "",
//!     "uploadUrl": "",
//!     "eventSourceUrl": "",
//!     "state": ""
//! }"#).unwrap();
//! let auth = SecretString::from("Bearer xyz");
//! let opts = JmapCalendarEventQueryOptions {
//!     filter: Some(JmapCalendarEventFilter {
//!         after: Some("2026-08-01T00:00:00".into()),
//!         before: Some("2026-09-01T00:00:00".into()),
//!         ..Default::default()
//!     }),
//!     expand_recurrences: true,
//!     ..Default::default()
//! };
//! let mut coroutine = JmapCalendarEventQuery::new(&session, &auth, opts).unwrap();
//! let mut arg = None;
//!
//! let out = loop {
//!     match coroutine.resume(arg.take()) {
//!         JmapCoroutineState::Yielded(JmapYield::WantsWrite(bytes)) => {
//!             stream.write_all(&bytes).unwrap();
//!         }
//!         JmapCoroutineState::Yielded(JmapYield::WantsRead) => {
//!             let n = stream.read(&mut buf).unwrap();
//!             arg = Some(&buf[..n]);
//!         }
//!         JmapCoroutineState::Complete(Ok(out)) => break out,
//!         JmapCoroutineState::Complete(Err(err)) => panic!("{err}"),
//!     }
//! };
//!
//! println!("{} events", out.events.len());
//! ```

use core::fmt;

use alloc::{string::String, vec, vec::Vec};

use secrecy::SecretString;
use serde::{Deserialize, Serialize, Serializer};
use thiserror::Error;

use crate::{
    calendars::{JMAP_CALENDARS_CAPABILITY, calendar_event::JmapCalendarEvent},
    coroutine::*,
    jmap_try,
    rfc8620::{
        JMAP_CORE_CAPABILITY,
        error::JmapMethodError,
        request::{JmapBatch, JmapResultReference},
        send::*,
        session::JmapSession,
    },
};

/// Filter for `CalendarEvent/query` (draft-ietf-jmap-calendars); all
/// specified conditions must apply.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapCalendarEventFilter {
    /// Calendar id the event must be in.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub in_calendar: Option<String>,
    /// The event must end after this local date-time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
    /// The event must start before this local date-time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    /// Free-text match against any text in the event.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Match against the event title.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Match against the event description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Match against any location name of the event.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    /// Match against the name or address of a participant with the
    /// owner role.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    /// Match against the name or address of any participant with the
    /// attendee role.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attendee: Option<String>,
    /// Exact JSCalendar `uid` of the event.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uid: Option<String>,
}

/// Sort property for `CalendarEvent/query` (draft-ietf-jmap-calendars).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JmapCalendarEventSortProperty {
    /// The event's start, in the query's time zone.
    Start,
    /// The JSCalendar `uid` of the event.
    Uid,
    /// The recurrence id of the occurrence, which orders the instances
    /// of a single series.
    RecurrenceId,
    /// The `created` date on the event.
    Created,
    /// The `updated` date on the event.
    Updated,
}

impl fmt::Display for JmapCalendarEventSortProperty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Start => "start",
            Self::Uid => "uid",
            Self::RecurrenceId => "recurrenceId",
            Self::Created => "created",
            Self::Updated => "updated",
        })
    }
}

impl Serialize for JmapCalendarEventSortProperty {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

/// Sort comparator for `CalendarEvent/query` (RFC 8620 §5.5).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapCalendarEventSortComparator {
    /// The property to sort by.
    pub property: JmapCalendarEventSortProperty,
    /// Ascending if `None` or `Some(true)`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_ascending: Option<bool>,
}

/// Failure causes during a batched JMAP `CalendarEvent/query` +
/// `CalendarEvent/get` flow.
#[derive(Debug, Error)]
pub enum JmapCalendarEventQueryError {
    /// The response carried no query response.
    #[error(
        "JMAP CalendarEvent/query failed: missing CalendarEvent/query response in method_responses"
    )]
    MissingQueryResponse,
    /// The response carried no get response.
    #[error(
        "JMAP CalendarEvent/query failed: missing CalendarEvent/get response in method_responses"
    )]
    MissingGetResponse,
    /// The inner send coroutine failed.
    #[error("JMAP CalendarEvent/query failed: {0}")]
    Send(#[from] JmapSendError),
    /// The method arguments could not be serialized.
    #[error("JMAP CalendarEvent/query failed: serialize args: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    /// The query response could not be parsed.
    #[error("JMAP CalendarEvent/query failed: parse CalendarEvent/query response: {0}")]
    ParseQueryResponse(#[source] serde_json::Error),
    /// The get response could not be parsed.
    #[error("JMAP CalendarEvent/query failed: parse CalendarEvent/get response: {0}")]
    ParseGetResponse(#[source] serde_json::Error),
    /// The server returned a method-level error for the query call.
    #[error("JMAP CalendarEvent/query failed: CalendarEvent/query: {0}")]
    QueryMethod(JmapMethodError),
    /// The server returned a method-level error for the get call.
    #[error("JMAP CalendarEvent/query failed: CalendarEvent/get: {0}")]
    GetMethod(JmapMethodError),
}

/// Options for [`JmapCalendarEventQuery::new`].
#[derive(Clone, Debug, Default)]
pub struct JmapCalendarEventQueryOptions {
    /// Filter criteria; `None` matches all events.
    pub filter: Option<JmapCalendarEventFilter>,
    /// Sort order; `None` uses the server default.
    pub sort: Option<Vec<JmapCalendarEventSortComparator>>,
    /// Zero-based offset into the result list.
    pub position: Option<u64>,
    /// Max number of events to return.
    pub limit: Option<u64>,
    /// Return one id per occurrence rather than one per series, which
    /// requires the filter to bound the window with `after` and
    /// `before`.
    pub expand_recurrences: bool,
    /// IANA time zone id the filter's and the returned floating times
    /// are resolved against; `None` leaves the server default
    /// (`Etc/UTC`).
    pub time_zone: Option<String>,
    /// Event properties to fetch (JSCalendar property names plus the
    /// JMAP ones); `None` returns all.
    pub properties: Option<Vec<String>>,
    /// Return only the participants the user needs to answer an
    /// invitation, i.e. themselves and the owners.
    pub reduce_participants: bool,
}

/// Successful terminal output of [`JmapCalendarEventQuery`].
#[derive(Clone, Debug)]
pub struct JmapCalendarEventQueryOutput {
    /// The fetched calendar events.
    pub events: Vec<JmapCalendarEvent>,
    /// The total number of matching objects, when the server computed
    /// it.
    pub total: Option<u64>,
    /// Zero-based index of the first returned id.
    pub position: u64,
    /// The state the query results were computed at.
    pub query_state: String,
    /// Whether the server indicated the connection can be reused.
    pub keep_alive: bool,
}

/// I/O-free coroutine for the combined `CalendarEvent/query` +
/// `CalendarEvent/get` operation.
pub struct JmapCalendarEventQuery {
    state: State,
}

impl JmapCalendarEventQuery {
    /// Prepares the method call request and builds the coroutine.
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        opts: JmapCalendarEventQueryOptions,
    ) -> Result<Self, JmapCalendarEventQueryError> {
        let account_id = session
            .primary_accounts
            .get(JMAP_CALENDARS_CAPABILITY)
            .cloned()
            .unwrap_or_default();
        let api_url = &session.api_url;

        let query_args = CalendarEventQueryArgs {
            account_id: &account_id,
            filter: opts.filter.as_ref(),
            sort: opts.sort.as_deref(),
            position: opts.position,
            limit: opts.limit,
            calculate_total: true,
            expand_recurrences: opts.expand_recurrences,
            time_zone: opts.time_zone.as_deref(),
        };

        let mut batch = JmapBatch::new();
        let query_id = batch.add(
            "CalendarEvent/query",
            serde_json::to_value(&query_args)
                .map_err(JmapCalendarEventQueryError::SerializeArgs)?,
        );

        let get_args = CalendarEventGetByRefArgs {
            account_id: &account_id,
            ids_ref: JmapResultReference {
                result_of: &query_id,
                name: "CalendarEvent/query",
                path: "/ids",
            },
            properties: opts.properties.as_deref(),
            reduce_participants: opts.reduce_participants,
            time_zone: opts.time_zone.as_deref(),
        };

        batch.add(
            "CalendarEvent/get",
            serde_json::to_value(&get_args).map_err(JmapCalendarEventQueryError::SerializeArgs)?,
        );

        let request = batch.into_request(vec![
            JMAP_CORE_CAPABILITY.into(),
            JMAP_CALENDARS_CAPABILITY.into(),
        ]);

        Ok(Self {
            state: State::Send(JmapSend::new(http_auth, api_url, request)?),
        })
    }
}

impl JmapCoroutine for JmapCalendarEventQuery {
    type Yield = JmapYield;
    type Return = Result<JmapCalendarEventQueryOutput, JmapCalendarEventQueryError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> JmapCoroutineState<Self::Yield, Self::Return> {
        match &mut self.state {
            State::Send(send) => {
                let JmapSendOutput {
                    response,
                    keep_alive,
                } = jmap_try!(send, arg);

                let mut responses = response.method_responses.into_iter();

                let Some((query_name, query_args, _)) = responses.next() else {
                    return JmapCoroutineState::Complete(Err(
                        JmapCalendarEventQueryError::MissingQueryResponse,
                    ));
                };

                if query_name == "error" {
                    let err = serde_json::from_value::<JmapMethodError>(query_args)
                        .unwrap_or(JmapMethodError::Unknown);
                    return JmapCoroutineState::Complete(Err(
                        JmapCalendarEventQueryError::QueryMethod(err),
                    ));
                }

                let query_response =
                    match serde_json::from_value::<CalendarEventQueryResponse>(query_args) {
                        Ok(r) => r,
                        Err(err) => {
                            return JmapCoroutineState::Complete(Err(
                                JmapCalendarEventQueryError::ParseQueryResponse(err),
                            ));
                        }
                    };

                let Some((get_name, get_args, _)) = responses.next() else {
                    return JmapCoroutineState::Complete(Err(
                        JmapCalendarEventQueryError::MissingGetResponse,
                    ));
                };

                if get_name == "error" {
                    let err = serde_json::from_value::<JmapMethodError>(get_args)
                        .unwrap_or(JmapMethodError::Unknown);
                    return JmapCoroutineState::Complete(Err(
                        JmapCalendarEventQueryError::GetMethod(err),
                    ));
                }

                match serde_json::from_value::<CalendarEventGetResponse>(get_args) {
                    Ok(r) => JmapCoroutineState::Complete(Ok(JmapCalendarEventQueryOutput {
                        events: r.list,
                        total: query_response.total,
                        position: query_response.position,
                        query_state: query_response.query_state,
                        keep_alive,
                    })),
                    Err(err) => JmapCoroutineState::Complete(Err(
                        JmapCalendarEventQueryError::ParseGetResponse(err),
                    )),
                }
            }
        }
    }
}

enum State {
    Send(JmapSend),
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CalendarEventQueryArgs<'a> {
    account_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    filter: Option<&'a JmapCalendarEventFilter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sort: Option<&'a [JmapCalendarEventSortComparator]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    position: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    limit: Option<u64>,
    calculate_total: bool,
    #[serde(skip_serializing_if = "is_false")]
    expand_recurrences: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    time_zone: Option<&'a str>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CalendarEventGetByRefArgs<'a> {
    account_id: &'a str,
    #[serde(rename = "#ids")]
    ids_ref: JmapResultReference<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    properties: Option<&'a [String]>,
    #[serde(skip_serializing_if = "is_false")]
    reduce_participants: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    time_zone: Option<&'a str>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CalendarEventQueryResponse {
    query_state: String,
    #[serde(default)]
    total: Option<u64>,
    #[serde(default)]
    position: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CalendarEventGetResponse {
    list: Vec<JmapCalendarEvent>,
}

fn is_false(b: &bool) -> bool {
    !b
}
