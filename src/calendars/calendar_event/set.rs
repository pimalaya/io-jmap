//! JMAP `CalendarEvent/set` coroutine (draft-ietf-jmap-calendars-27):
//! builds its own method call, `sendSchedulingMessages` being an argument
//! the generic [`JmapSet`] has no notion of, then decodes the response
//! through it, the per-object errors as [`JmapCalendarEventSetItemError`].
//!
//! An update is a PatchObject (RFC 8620 §5.3), so a full replacement is a
//! patch naming every top-level property, `null` for those to remove. A
//! patch reaching inside `recurrenceOverrides` lands in a PatchObject of
//! its own, where a `null` removes rather than sets: an override carrying
//! a `null` has to be sent whole, as the draft's patching section shows.
//!
//! # Example
//!
//! ```rust,no_run
//! use std::{
//!     collections::BTreeMap,
//!     io::{Read, Write},
//!     net::TcpStream,
//! };
//!
//! use io_jmap::{
//!     calendars::calendar_event::set::{JmapCalendarEventPatch, JmapCalendarEventSet, JmapCalendarEventSetArgs},
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
//! let patch = JmapCalendarEventPatch(BTreeMap::from([("title".into(), "Standup".into())]));
//! let args = JmapCalendarEventSetArgs {
//!     if_in_state: Some("s1".into()),
//!     update: Some(BTreeMap::from([("e1".into(), patch)])),
//!     ..Default::default()
//! };
//! let mut coroutine = JmapCalendarEventSet::new(&session, &auth, args).unwrap();
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
//! println!("new state {}", out.new_state);
//! ```

use core::fmt;

use alloc::{collections::BTreeMap, string::String, vec, vec::Vec};

use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    calendars::{JMAP_CALENDARS_CAPABILITY, calendar_event::JmapCalendarEvent},
    coroutine::*,
    jmap_try,
    rfc8620::{
        JMAP_CORE_CAPABILITY,
        error::{write_described, write_properties},
        request::JmapBatch,
        send::*,
        session::JmapSession,
        set::*,
    },
};

/// Patch object for `CalendarEvent/set` update requests (RFC 8620 §5.3):
/// JSON pointer paths mapped to their new values; a `null` value removes
/// the pointed property.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(transparent)]
pub struct JmapCalendarEventPatch(pub BTreeMap<String, serde_json::Value>);

/// Per-object error returned in `CalendarEvent/set` responses.
///
/// NOTE: variants keep the struct shape even with a single field, the
/// internally-tagged serde representation of the wire object requires
/// it.
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum JmapCalendarEventSetItemError {
    /// Standard set error (RFC 8620 §5.3): the change is not allowed, e.g.
    /// by the calendar's `myRights` or a `private` event of another user.
    Forbidden {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// Standard set error (RFC 8620 §5.3): the change would exceed the
    /// account's storage quota.
    OverQuota {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// Standard set error (RFC 8620 §5.3): the event is larger than the
    /// server takes.
    TooLarge {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// Standard set error (RFC 8620 §5.3): too many objects written
    /// lately, to try again later.
    RateLimit {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// Standard set error (RFC 8620 §5.3): target id not found.
    NotFound {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// Standard set error (RFC 8620 §5.3): patch could not be applied.
    InvalidPatch {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// Standard set error (RFC 8620 §5.3): would update an object already
    /// queued for destruction in the same request.
    WillDestroy {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// Standard set error (RFC 8620 §5.3): one or more properties were
    /// invalid, e.g. a `method`, or `utcStart` beside `start`.
    InvalidProperties {
        /// Optional human-readable detail.
        description: Option<String>,
        /// The invalid property names.
        #[serde(default)]
        properties: Vec<String>,
    },
    /// Standard set error (RFC 8620 §5.3): tried to create or destroy a
    /// server-managed singleton.
    Singleton {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// Scheduling messages were asked for, but the server supports none of
    /// the methods available for at least one recipient.
    NoSupportedScheduleMethods {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// Catch-all for set errors not modelled above.
    #[serde(other)]
    Unknown,
}

impl fmt::Display for JmapCalendarEventSetItemError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Forbidden { description } => write_described(f, "forbidden", description),
            Self::OverQuota { description } => write_described(f, "overQuota", description),
            Self::TooLarge { description } => write_described(f, "tooLarge", description),
            Self::RateLimit { description } => write_described(f, "rateLimit", description),
            Self::NotFound { description } => write_described(f, "notFound", description),
            Self::InvalidPatch { description } => write_described(f, "invalidPatch", description),
            Self::WillDestroy { description } => write_described(f, "willDestroy", description),
            Self::InvalidProperties {
                description,
                properties,
            } => write_properties(f, description, properties),
            Self::Singleton { description } => write_described(f, "singleton", description),
            Self::NoSupportedScheduleMethods { description } => {
                write_described(f, "noSupportedScheduleMethods", description)
            }
            Self::Unknown => write!(f, "JMAP unknown error"),
        }
    }
}

/// Failure causes during a JMAP `CalendarEvent/set` flow.
#[derive(Debug, Error)]
pub enum JmapCalendarEventSetError {
    /// The inner send coroutine failed.
    #[error("JMAP CalendarEvent/set failed: {0}")]
    Send(#[from] JmapSendError),
    /// The method arguments could not be serialized.
    #[error("JMAP CalendarEvent/set failed: serialize args: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    /// The inner generic set coroutine failed, a `stateMismatch` among
    /// its method errors.
    #[error("JMAP CalendarEvent/set failed: {0}")]
    Set(#[from] JmapSetError),
}

/// Arguments for a `CalendarEvent/set` request.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapCalendarEventSetArgs {
    /// Optimistic concurrency token: the call is refused with
    /// `stateMismatch` and nothing changes unless the server's
    /// CalendarEvent state is still this one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub if_in_state: Option<String>,
    /// Objects to create (client ID → CalendarEvent, with its
    /// `calendarIds` and no server-set property).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create: Option<BTreeMap<String, JmapCalendarEvent>>,
    /// Objects to update (CalendarEvent ID → patch object); a synthetic
    /// id of an expanded occurrence patches that occurrence's override.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update: Option<BTreeMap<String, JmapCalendarEventPatch>>,
    /// IDs of objects to destroy; a synthetic id of an expanded
    /// occurrence removes that occurrence alone.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destroy: Option<Vec<String>>,
    /// Whether the server sends the iTIP messages the changes call for,
    /// to the participants when it is the event's origin, to the
    /// organizer otherwise; none when false (the draft's default).
    #[serde(skip_serializing_if = "is_false")]
    pub send_scheduling_messages: bool,
}

/// Successful terminal output of [`JmapCalendarEventSet`].
#[derive(Clone, Debug)]
pub struct JmapCalendarEventSetOutput {
    /// The new server state after the call; empty when the server named
    /// none.
    pub new_state: String,
    /// The created events, keyed by client id, carrying the properties
    /// the server set (the id among them).
    pub created: BTreeMap<String, JmapCalendarEvent>,
    /// The updated events, keyed by id, carrying the properties the
    /// server changed beyond the patch when it says so.
    pub updated: BTreeMap<String, Option<JmapCalendarEvent>>,
    /// Ids of the destroyed objects.
    pub destroyed: Vec<String>,
    /// The failed creates, keyed by client id.
    pub not_created: BTreeMap<String, JmapCalendarEventSetItemError>,
    /// The failed updates, keyed by id.
    pub not_updated: BTreeMap<String, JmapCalendarEventSetItemError>,
    /// The failed destroys, keyed by id.
    pub not_destroyed: BTreeMap<String, JmapCalendarEventSetItemError>,
    /// Whether the server indicated the connection can be reused.
    pub keep_alive: bool,
}

/// I/O-free coroutine for the JMAP `CalendarEvent/set` method.
pub struct JmapCalendarEventSet {
    state: State,
}

impl JmapCalendarEventSet {
    /// Prepares the method call request and builds the coroutine.
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        args: JmapCalendarEventSetArgs,
    ) -> Result<Self, JmapCalendarEventSetError> {
        let account_id = session
            .primary_accounts
            .get(JMAP_CALENDARS_CAPABILITY)
            .cloned()
            .unwrap_or_default();
        let api_url = &session.api_url;

        let json_args = serde_json::to_value(CalendarEventSetRequest { account_id, args })
            .map_err(JmapCalendarEventSetError::SerializeArgs)?;

        let mut batch = JmapBatch::new();
        batch.add("CalendarEvent/set", json_args);
        let request = batch.into_request(vec![
            JMAP_CORE_CAPABILITY.into(),
            JMAP_CALENDARS_CAPABILITY.into(),
        ]);

        let send = JmapSend::new(http_auth, api_url, request)?;
        Ok(Self {
            state: State::Set(JmapSet::from_send(send)),
        })
    }
}

impl JmapCoroutine for JmapCalendarEventSet {
    type Yield = JmapYield;
    type Return = Result<JmapCalendarEventSetOutput, JmapCalendarEventSetError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> JmapCoroutineState<Self::Yield, Self::Return> {
        match &mut self.state {
            State::Set(set) => {
                let JmapSetOutput {
                    new_state,
                    created,
                    updated,
                    destroyed,
                    not_created,
                    not_updated,
                    not_destroyed,
                    keep_alive,
                } = jmap_try!(set, arg);
                let parse = |map: BTreeMap<String, serde_json::Value>| {
                    map.into_iter()
                        .map(|(k, v)| {
                            let e = serde_json::from_value(v)
                                .unwrap_or(JmapCalendarEventSetItemError::Unknown);
                            (k, e)
                        })
                        .collect()
                };
                JmapCoroutineState::Complete(Ok(JmapCalendarEventSetOutput {
                    new_state,
                    created,
                    updated,
                    destroyed,
                    not_created: parse(not_created),
                    not_updated: parse(not_updated),
                    not_destroyed: parse(not_destroyed),
                    keep_alive,
                }))
            }
        }
    }
}

enum State {
    Set(JmapSet<JmapCalendarEvent>),
}

#[derive(Serialize)]
struct CalendarEventSetRequest {
    #[serde(rename = "accountId")]
    account_id: String,
    #[serde(flatten)]
    args: JmapCalendarEventSetArgs,
}

fn is_false(b: &bool) -> bool {
    !b
}
