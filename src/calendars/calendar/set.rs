//! JMAP `Calendar/set` coroutine: wraps the generic [`JmapSet`] with
//! [`JmapCalendarSetArgs`] (create/update/destroy plus the
//! `onDestroyRemoveEvents` and `onSuccessSetIsDefault` extra arguments)
//! and decodes per-object [`JmapCalendarSetItemError`] payloads.
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
//!     calendars::calendar::set::{JmapCalendarSet, JmapCalendarSetArgs},
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
//! let mut coroutine =
//!     JmapCalendarSet::new(&session, &auth, JmapCalendarSetArgs::default()).unwrap();
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
    calendars::{
        JMAP_CALENDARS_CAPABILITY,
        calendar::{JmapCalendar, JmapCalendarAvailability, JmapCalendarRights},
    },
    coroutine::*,
    jmap_try,
    rfc8620::{
        JMAP_CORE_CAPABILITY,
        error::{write_described, write_properties},
        request::JmapBatch,
        send::*,
        session::{JmapSession, JmapSessionAccountError},
        set::*,
    },
};

/// Client-settable subset of [`JmapCalendar`] for `Calendar/set` create
/// requests. Server-set fields are excluded.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapCalendarCreate {
    /// The user-visible calendar name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Optional long-form description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Colour to display the calendar's events with, as a CSS colour.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Position hint for display ordering (lower first).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_order: Option<u32>,
    /// Whether the user is subscribed to the calendar.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_subscribed: Option<bool>,
    /// Whether the calendar's events are displayed by default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_visible: Option<bool>,
    /// Which of the calendar's events count towards availability.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_in_availability: Option<JmapCalendarAvailability>,
    /// IANA time zone id floating times are resolved against.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_zone: Option<String>,
    /// Principal id to rights map (RFC 9670).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub share_with: Option<BTreeMap<String, JmapCalendarRights>>,
}

/// Patch object for `Calendar/set` update requests (RFC 8620 §5.3).
///
/// A `None` field is left unchanged; on a nullable property `Some(None)`
/// clears it (`null`).
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapCalendarUpdate {
    /// The user-visible calendar name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Optional long-form description; `Some(None)` clears it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<Option<String>>,
    /// CSS colour; `Some(None)` clears it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<Option<String>>,
    /// Position hint for display ordering (lower first).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_order: Option<u32>,
    /// Whether the user is subscribed to the calendar.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_subscribed: Option<bool>,
    /// Whether the calendar's events are displayed by default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_visible: Option<bool>,
    /// Which of the calendar's events count towards availability.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_in_availability: Option<JmapCalendarAvailability>,
    /// IANA time zone id; `Some(None)` falls back to the account's.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_zone: Option<Option<String>>,
    /// Principal id to rights map (RFC 9670); `Some(None)` unshares it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub share_with: Option<Option<BTreeMap<String, JmapCalendarRights>>>,
}

/// Per-object error returned in `Calendar/set` responses.
///
/// Covers the standard RFC 8620 §5.3 set errors plus the draft's
/// `calendarHasEvent`.
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum JmapCalendarSetItemError {
    /// The calendar still holds events and `onDestroyRemoveEvents` was
    /// false.
    CalendarHasEvent {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// Standard set error (RFC 8620 §5.3): the change is not allowed.
    Forbidden {
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
    /// Standard set error (RFC 8620 §5.3): would destroy an object
    /// already queued for destruction in the same request.
    WillDestroy {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// Standard set error (RFC 8620 §5.3): one or more properties were
    /// invalid.
    InvalidProperties {
        /// Optional human-readable detail.
        description: Option<String>,
        /// The invalid property names.
        #[serde(default)]
        properties: Vec<String>,
    },
    /// A set error type this library does not model, kept as the server
    /// named it.
    #[serde(untagged)]
    Other {
        /// The error type, empty when the server named none.
        #[serde(rename = "type")]
        type_name: String,
        /// Optional human-readable detail.
        description: Option<String>,
    },
}

impl fmt::Display for JmapCalendarSetItemError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CalendarHasEvent { description } => {
                write_described(f, "calendarHasEvent", description)
            }
            Self::Forbidden { description } => write_described(f, "forbidden", description),
            Self::NotFound { description } => write_described(f, "notFound", description),
            Self::InvalidPatch { description } => write_described(f, "invalidPatch", description),
            Self::WillDestroy { description } => write_described(f, "willDestroy", description),
            Self::InvalidProperties {
                description,
                properties,
            } => write_properties(f, description, properties),
            Self::Other {
                type_name,
                description,
            } => write_described(f, type_name, description),
        }
    }
}

/// Failure causes during a JMAP `Calendar/set` flow.
#[derive(Debug, Error)]
pub enum JmapCalendarSetError {
    /// The session names no account to address the call to.
    #[error("JMAP Calendar/set failed: {0}")]
    Account(#[from] JmapSessionAccountError),
    /// The inner send coroutine failed.
    #[error("JMAP Calendar/set failed: {0}")]
    Send(#[from] JmapSendError),
    /// The method arguments could not be serialized.
    #[error("JMAP Calendar/set failed: serialize args: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    /// The inner generic set coroutine failed.
    #[error("JMAP Calendar/set failed: {0}")]
    Set(#[from] JmapSetError),
}

/// Arguments for a `Calendar/set` request.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapCalendarSetArgs {
    /// Objects to create (client ID → partial Calendar object).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create: Option<BTreeMap<String, JmapCalendarCreate>>,
    /// Objects to update (Calendar ID → patch object).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update: Option<BTreeMap<String, JmapCalendarUpdate>>,
    /// IDs of objects to destroy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destroy: Option<Vec<String>>,
    /// Whether to remove the events of a destroyed calendar; an event
    /// left in no calendar is destroyed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_destroy_remove_events: Option<bool>,
    /// Calendar ID (or `#`-prefixed creation ID) to make the default
    /// when all changes succeed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_success_set_is_default: Option<String>,
}

/// Successful terminal output of [`JmapCalendarSet`].
#[derive(Clone, Debug)]
pub struct JmapCalendarSetOutput {
    /// The new server state after the call.
    pub new_state: String,
    /// The created calendars, keyed by client id.
    pub created: BTreeMap<String, JmapCalendar>,
    /// The updated calendars, keyed by id.
    pub updated: BTreeMap<String, Option<JmapCalendar>>,
    /// Ids of the destroyed objects.
    pub destroyed: Vec<String>,
    /// The failed creates, keyed by client id.
    pub not_created: BTreeMap<String, JmapCalendarSetItemError>,
    /// The failed updates, keyed by id.
    pub not_updated: BTreeMap<String, JmapCalendarSetItemError>,
    /// The failed destroys, keyed by id.
    pub not_destroyed: BTreeMap<String, JmapCalendarSetItemError>,
    /// Whether the server indicated the connection can be reused.
    pub keep_alive: bool,
}

/// I/O-free coroutine for the JMAP `Calendar/set` method.
pub struct JmapCalendarSet {
    state: State,
}

impl JmapCalendarSet {
    /// Prepares the method call request and builds the coroutine.
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        args: JmapCalendarSetArgs,
    ) -> Result<Self, JmapCalendarSetError> {
        let account_id = String::from(session.account_for(JMAP_CALENDARS_CAPABILITY)?);
        let api_url = &session.api_url;

        let json_args = serde_json::to_value(CalendarSetRequest { account_id, args })
            .map_err(JmapCalendarSetError::SerializeArgs)?;

        let mut batch = JmapBatch::new();
        batch.add("Calendar/set", json_args);
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

impl JmapCoroutine for JmapCalendarSet {
    type Yield = JmapYield;
    type Return = Result<JmapCalendarSetOutput, JmapCalendarSetError>;

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
                            let e = serde_json::from_value(v).unwrap_or(
                                JmapCalendarSetItemError::Other {
                                    type_name: String::new(),
                                    description: None,
                                },
                            );
                            (k, e)
                        })
                        .collect()
                };
                JmapCoroutineState::Complete(Ok(JmapCalendarSetOutput {
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
    Set(JmapSet<JmapCalendar>),
}

#[derive(Serialize)]
struct CalendarSetRequest {
    #[serde(rename = "accountId")]
    account_id: String,
    #[serde(flatten)]
    args: JmapCalendarSetArgs,
}

#[cfg(test)]
mod tests {
    use alloc::string::ToString;

    use serde_json::{from_value, json};

    use crate::calendars::calendar::set::{JmapCalendarSetItemError, JmapCalendarUpdate};

    #[test]
    fn update_clears_a_nullable_property() {
        let patch = JmapCalendarUpdate {
            description: Some(None),
            color: Some(None),
            ..Default::default()
        };

        assert_eq!(
            serde_json::to_value(patch).unwrap(),
            json!({ "description": null, "color": null })
        );
    }

    #[test]
    fn a_refusal_reads_as_its_type_and_description() {
        let refused: JmapCalendarSetItemError = from_value(json!({
            "type": "calendarHasEvent",
            "description": "d",
        }))
        .unwrap();

        assert!(matches!(
            refused,
            JmapCalendarSetItemError::CalendarHasEvent { .. }
        ));
        assert_eq!(refused.to_string(), "JMAP calendarHasEvent: d");
    }

    #[test]
    fn an_unmodelled_error_keeps_its_type_and_description() {
        let refused: JmapCalendarSetItemError =
            from_value(json!({ "type": "overQuota", "description": "d" })).unwrap();

        assert!(matches!(
            &refused,
            JmapCalendarSetItemError::Other { type_name, .. } if type_name == "overQuota"
        ));
        assert_eq!(refused.to_string(), "JMAP overQuota: d");
    }
}
