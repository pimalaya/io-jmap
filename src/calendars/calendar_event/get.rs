//! JMAP `CalendarEvent/get` coroutine (draft-ietf-jmap-calendars-27):
//! builds its own method call, the recurrence-expansion window and the
//! participant reduction being arguments the generic [`JmapGet`] has no
//! notion of, then decodes the response through it.
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
//!     calendars::calendar_event::get::{JmapCalendarEventGet, JmapCalendarEventGetOptions},
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
//! let opts = JmapCalendarEventGetOptions {
//!     ids: Some(vec!["e1".into()]),
//!     ..Default::default()
//! };
//! let mut coroutine = JmapCalendarEventGet::new(&session, &auth, opts).unwrap();
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

use alloc::{string::String, vec, vec::Vec};

use secrecy::SecretString;
use serde::Serialize;
use thiserror::Error;

use crate::{
    calendars::{JMAP_CALENDARS_CAPABILITY, calendar_event::JmapCalendarEvent},
    coroutine::*,
    jmap_try,
    rfc8620::{JMAP_CORE_CAPABILITY, get::*, request::JmapBatch, send::*, session::JmapSession},
};

/// Failure causes during a JMAP `CalendarEvent/get` flow.
#[derive(Debug, Error)]
pub enum JmapCalendarEventGetError {
    /// The inner send coroutine failed.
    #[error("JMAP CalendarEvent/get failed: {0}")]
    Send(#[from] JmapSendError),
    /// The method arguments could not be serialized.
    #[error("JMAP CalendarEvent/get failed: serialize args: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    /// The inner generic get coroutine failed.
    #[error("JMAP CalendarEvent/get failed: {0}")]
    Get(#[from] JmapGetError),
}

/// Options for [`JmapCalendarEventGet::new`].
#[derive(Clone, Debug, Default)]
pub struct JmapCalendarEventGetOptions {
    /// Restrict the fetch to these CalendarEvent IDs; `None` fetches
    /// all.
    pub ids: Option<Vec<String>>,
    /// Restrict the returned properties (JSCalendar property names plus
    /// the JMAP ones); `None` returns all.
    pub properties: Option<Vec<String>>,
    /// Drop the recurrence overrides starting at or after this UTC
    /// date-time, keeping a fetch of a long-running series small.
    pub recurrence_overrides_before: Option<String>,
    /// Drop the recurrence overrides ending before this UTC date-time.
    pub recurrence_overrides_after: Option<String>,
    /// Return only the participants the user needs to answer an
    /// invitation, i.e. themselves and the owners.
    pub reduce_participants: bool,
    /// IANA time zone id the returned floating times are resolved
    /// against; `None` leaves the server default (`Etc/UTC`).
    pub time_zone: Option<String>,
}

/// Successful terminal output of [`JmapCalendarEventGet`].
#[derive(Clone, Debug)]
pub struct JmapCalendarEventGetOutput {
    /// The fetched calendar events.
    pub events: Vec<JmapCalendarEvent>,
    /// The requested ids the server did not find.
    pub not_found: Vec<String>,
    /// The new server state after the call.
    pub new_state: String,
    /// Whether the server indicated the connection can be reused.
    pub keep_alive: bool,
}

/// I/O-free coroutine for the JMAP `CalendarEvent/get` method.
pub struct JmapCalendarEventGet {
    state: State,
}

impl JmapCalendarEventGet {
    /// Prepares the method call request and builds the coroutine.
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        opts: JmapCalendarEventGetOptions,
    ) -> Result<Self, JmapCalendarEventGetError> {
        let account_id = session
            .primary_accounts
            .get(JMAP_CALENDARS_CAPABILITY)
            .cloned()
            .unwrap_or_default();
        let api_url = &session.api_url;

        let args = serde_json::to_value(CalendarEventGetArgs {
            account_id: &account_id,
            ids: opts.ids.as_deref(),
            properties: opts.properties.as_deref(),
            recurrence_overrides_before: opts.recurrence_overrides_before.as_deref(),
            recurrence_overrides_after: opts.recurrence_overrides_after.as_deref(),
            reduce_participants: opts.reduce_participants,
            time_zone: opts.time_zone.as_deref(),
        })
        .map_err(JmapCalendarEventGetError::SerializeArgs)?;

        let mut batch = JmapBatch::new();
        batch.add("CalendarEvent/get", args);
        let request = batch.into_request(vec![
            JMAP_CORE_CAPABILITY.into(),
            JMAP_CALENDARS_CAPABILITY.into(),
        ]);

        let send = JmapSend::new(http_auth, api_url, request)?;

        Ok(Self {
            state: State::Get(JmapGet::from_send(send)),
        })
    }
}

impl JmapCoroutine for JmapCalendarEventGet {
    type Yield = JmapYield;
    type Return = Result<JmapCalendarEventGetOutput, JmapCalendarEventGetError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> JmapCoroutineState<Self::Yield, Self::Return> {
        match &mut self.state {
            State::Get(get) => {
                let JmapGetOutput {
                    list,
                    not_found,
                    state,
                    keep_alive,
                } = jmap_try!(get, arg);
                JmapCoroutineState::Complete(Ok(JmapCalendarEventGetOutput {
                    events: list,
                    not_found,
                    new_state: state,
                    keep_alive,
                }))
            }
        }
    }
}

enum State {
    Get(JmapGet<JmapCalendarEvent>),
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CalendarEventGetArgs<'a> {
    account_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    ids: Option<&'a [String]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    properties: Option<&'a [String]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    recurrence_overrides_before: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    recurrence_overrides_after: Option<&'a str>,
    #[serde(skip_serializing_if = "is_false")]
    reduce_participants: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    time_zone: Option<&'a str>,
}

fn is_false(b: &bool) -> bool {
    !b
}
