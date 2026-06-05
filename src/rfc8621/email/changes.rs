//! JMAP `Email/changes` coroutine (RFC 8621 §4.3): wraps the generic
//! [`JmapChanges`] with the JMAP-Mail capability set.
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
//!     coroutine::{JmapCoroutine, JmapCoroutineState, JmapYield},
//!     rfc8620::JmapSession,
//!     rfc8621::email::changes::{JmapEmailChanges, JmapEmailChangesOptions},
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
//!     "primaryAccounts": {"urn:ietf:params:jmap:mail": "a1"},
//!     "capabilities": {},
//!     "apiUrl": "https://api.example.com/jmap/",
//!     "downloadUrl": "",
//!     "uploadUrl": "",
//!     "eventSourceUrl": "",
//!     "state": ""
//! }"#).unwrap();
//! let auth = SecretString::from("Bearer xyz");
//! let mut coroutine =
//!     JmapEmailChanges::new(&session, &auth, "s1", JmapEmailChangesOptions::default()).unwrap();
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

use alloc::{string::String, vec};

use log::trace;
use secrecy::SecretString;
use thiserror::Error;

use crate::{
    coroutine::*,
    jmap_try,
    rfc8620::{CORE_CAPABILITY, JmapSession, changes::*},
    rfc8621::MAIL_CAPABILITY,
};

/// Failure causes during a JMAP `Email/changes` flow.
#[derive(Debug, Error)]
pub enum JmapEmailChangesError {
    #[error("JMAP Email/changes failed: {0}")]
    Changes(#[from] JmapChangesError),
}

/// Options for [`JmapEmailChanges::new`].
#[derive(Clone, Debug, Default)]
pub struct JmapEmailChangesOptions {
    /// Server-side cap on the number of changes returned.
    pub max_changes: Option<u64>,
}

/// I/O-free coroutine for the JMAP `Email/changes` method.
pub struct JmapEmailChanges {
    state: State,
}

impl JmapEmailChanges {
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        since_state: impl Into<String>,
        opts: JmapEmailChangesOptions,
    ) -> Result<Self, JmapEmailChangesError> {
        let account_id = session
            .primary_accounts
            .get(MAIL_CAPABILITY)
            .cloned()
            .unwrap_or_default();
        let api_url = &session.api_url;

        Ok(Self {
            state: State::Changes(JmapChanges::new(
                account_id,
                http_auth,
                api_url,
                "Email/changes",
                vec![CORE_CAPABILITY.into(), MAIL_CAPABILITY.into()],
                since_state,
                JmapChangesOptions {
                    max_changes: opts.max_changes,
                },
            )?),
        })
    }
}

impl JmapCoroutine for JmapEmailChanges {
    type Yield = JmapYield;
    type Return = Result<JmapChangesOutput, JmapEmailChangesError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> JmapCoroutineState<Self::Yield, Self::Return> {
        trace!("Email/changes: {}", self.state);
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

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Changes(_) => f.write_str("changes"),
        }
    }
}
