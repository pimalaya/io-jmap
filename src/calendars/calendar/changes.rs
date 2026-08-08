//! JMAP `Calendar/changes` coroutine (draft-ietf-jmap-calendars-27):
//! wraps the generic [`JmapChanges`] with the JMAP-Calendars capability
//! set.
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
//!     calendars::calendar::changes::{JmapCalendarChanges, JmapCalendarChangesOptions},
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
//!     JmapCalendarChanges::new(&session, &auth, "s1", JmapCalendarChangesOptions::default())
//!         .unwrap();
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

use alloc::{string::String, vec};

use secrecy::SecretString;
use thiserror::Error;

use crate::{
    calendars::JMAP_CALENDARS_CAPABILITY,
    coroutine::*,
    jmap_try,
    rfc8620::{JMAP_CORE_CAPABILITY, changes::*, session::JmapSession},
};

/// Failure causes during a JMAP `Calendar/changes` flow.
#[derive(Debug, Error)]
pub enum JmapCalendarChangesError {
    /// The inner generic changes coroutine failed.
    #[error("JMAP Calendar/changes failed: {0}")]
    Changes(#[from] JmapChangesError),
}

/// Options for [`JmapCalendarChanges::new`].
#[derive(Clone, Debug, Default)]
pub struct JmapCalendarChangesOptions {
    /// Server-side cap on the number of changes returned.
    pub max_changes: Option<u64>,
}

/// I/O-free coroutine for the JMAP `Calendar/changes` method.
pub struct JmapCalendarChanges {
    state: State,
}

impl JmapCalendarChanges {
    /// Prepares the method call request and builds the coroutine.
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        since_state: impl Into<String>,
        opts: JmapCalendarChangesOptions,
    ) -> Result<Self, JmapCalendarChangesError> {
        let account_id = session
            .primary_accounts
            .get(JMAP_CALENDARS_CAPABILITY)
            .cloned()
            .unwrap_or_default();
        let api_url = &session.api_url;

        Ok(Self {
            state: State::Changes(JmapChanges::new(
                account_id,
                http_auth,
                api_url,
                "Calendar/changes",
                vec![
                    JMAP_CORE_CAPABILITY.into(),
                    JMAP_CALENDARS_CAPABILITY.into(),
                ],
                since_state,
                JmapChangesOptions {
                    max_changes: opts.max_changes,
                },
            )?),
        })
    }
}

impl JmapCoroutine for JmapCalendarChanges {
    type Yield = JmapYield;
    type Return = Result<JmapChangesOutput, JmapCalendarChangesError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> JmapCoroutineState<Self::Yield, Self::Return> {
        match &mut self.state {
            State::Changes(changes) => {
                let out = jmap_try!(changes, arg);
                JmapCoroutineState::Complete(Ok(out))
            }
        }
    }
}

enum State {
    Changes(JmapChanges),
}
