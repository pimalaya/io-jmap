//! JMAP `Calendar/get` coroutine (draft-ietf-jmap-calendars-27): wraps
//! the generic [`JmapGet`] with the JMAP-Calendars capability set.
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
//!     calendars::calendar::get::{JmapCalendarGet, JmapCalendarGetOptions},
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
//!     JmapCalendarGet::new(&session, &auth, JmapCalendarGetOptions::default()).unwrap();
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
//! println!("{} calendars", out.calendars.len());
//! ```

use core::fmt;

use alloc::{
    string::{String, ToString},
    vec,
    vec::Vec,
};

use secrecy::SecretString;
use serde::{Serialize, Serializer};
use thiserror::Error;

use crate::{
    calendars::{JMAP_CALENDARS_CAPABILITY, calendar::JmapCalendar},
    coroutine::*,
    jmap_try,
    rfc8620::{JMAP_CORE_CAPABILITY, get::*, session::JmapSession},
};

/// [`JmapCalendar`] properties requestable in `Calendar/get`
/// (draft-ietf-jmap-calendars).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JmapCalendarProperty {
    /// The `id` property.
    Id,
    /// The `name` property.
    Name,
    /// The `description` property.
    Description,
    /// The `color` property.
    Color,
    /// The `sortOrder` property.
    SortOrder,
    /// The `isDefault` property.
    IsDefault,
    /// The `isSubscribed` property.
    IsSubscribed,
    /// The `isVisible` property.
    IsVisible,
    /// The `includeInAvailability` property.
    IncludeInAvailability,
    /// The `defaultAlertsWithTime` property.
    DefaultAlertsWithTime,
    /// The `defaultAlertsWithoutTime` property.
    DefaultAlertsWithoutTime,
    /// The `timeZone` property.
    TimeZone,
    /// The `shareWith` property.
    ShareWith,
    /// The `myRights` property.
    MyRights,
}

impl fmt::Display for JmapCalendarProperty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Id => "id",
            Self::Name => "name",
            Self::Description => "description",
            Self::Color => "color",
            Self::SortOrder => "sortOrder",
            Self::IsDefault => "isDefault",
            Self::IsSubscribed => "isSubscribed",
            Self::IsVisible => "isVisible",
            Self::IncludeInAvailability => "includeInAvailability",
            Self::DefaultAlertsWithTime => "defaultAlertsWithTime",
            Self::DefaultAlertsWithoutTime => "defaultAlertsWithoutTime",
            Self::TimeZone => "timeZone",
            Self::ShareWith => "shareWith",
            Self::MyRights => "myRights",
        })
    }
}

impl Serialize for JmapCalendarProperty {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

/// Failure causes during a JMAP `Calendar/get` flow.
#[derive(Debug, Error)]
pub enum JmapCalendarGetError {
    /// The inner generic get coroutine failed.
    #[error("JMAP Calendar/get failed: {0}")]
    Get(#[from] JmapGetError),
}

/// Options for [`JmapCalendarGet::new`].
#[derive(Clone, Debug, Default)]
pub struct JmapCalendarGetOptions {
    /// Restrict the fetch to these Calendar IDs; `None` fetches all.
    pub ids: Option<Vec<String>>,
    /// Restrict the returned properties; `None` returns all.
    pub properties: Option<Vec<JmapCalendarProperty>>,
}

/// Successful terminal output of [`JmapCalendarGet`].
#[derive(Clone, Debug)]
pub struct JmapCalendarGetOutput {
    /// The fetched calendars.
    pub calendars: Vec<JmapCalendar>,
    /// The requested ids the server did not find.
    pub not_found: Vec<String>,
    /// The new server state after the call.
    pub new_state: String,
    /// Whether the server indicated the connection can be reused.
    pub keep_alive: bool,
}

/// I/O-free coroutine for the JMAP `Calendar/get` method.
pub struct JmapCalendarGet {
    state: State,
}

impl JmapCalendarGet {
    /// Prepares the method call request and builds the coroutine.
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        opts: JmapCalendarGetOptions,
    ) -> Result<Self, JmapCalendarGetError> {
        let account_id = session
            .primary_accounts
            .get(JMAP_CALENDARS_CAPABILITY)
            .cloned()
            .unwrap_or_default();
        let api_url = &session.api_url;

        let properties = opts
            .properties
            .map(|props| props.iter().map(ToString::to_string).collect());

        Ok(Self {
            state: State::Get(JmapGet::new(
                account_id,
                http_auth,
                api_url,
                "Calendar/get",
                vec![
                    JMAP_CORE_CAPABILITY.into(),
                    JMAP_CALENDARS_CAPABILITY.into(),
                ],
                JmapGetOptions {
                    ids: opts.ids,
                    properties,
                },
            )?),
        })
    }
}

impl JmapCoroutine for JmapCalendarGet {
    type Yield = JmapYield;
    type Return = Result<JmapCalendarGetOutput, JmapCalendarGetError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> JmapCoroutineState<Self::Yield, Self::Return> {
        match &mut self.state {
            State::Get(get) => {
                let JmapGetOutput {
                    list,
                    not_found,
                    state,
                    keep_alive,
                } = jmap_try!(get, arg);
                JmapCoroutineState::Complete(Ok(JmapCalendarGetOutput {
                    calendars: list,
                    not_found,
                    new_state: state,
                    keep_alive,
                }))
            }
        }
    }
}

enum State {
    Get(JmapGet<JmapCalendar>),
}
