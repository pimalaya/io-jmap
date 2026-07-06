//! JMAP `ContactCard/changes` coroutine (RFC 9610 §3.2): wraps the generic
//! [`JmapChanges`] with the JMAP-Contacts capability set.
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
//!     rfc9610::contact_card::changes::{JmapContactCardChanges, JmapContactCardChangesOptions},
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
//!     "primaryAccounts": {"urn:ietf:params:jmap:contacts": "a1"},
//!     "capabilities": {},
//!     "apiUrl": "https://api.example.com/jmap/",
//!     "downloadUrl": "",
//!     "uploadUrl": "",
//!     "eventSourceUrl": "",
//!     "state": ""
//! }"#).unwrap();
//! let auth = SecretString::from("Bearer xyz");
//! let mut coroutine = JmapContactCardChanges::new(
//!     &session,
//!     &auth,
//!     "s1",
//!     JmapContactCardChangesOptions::default(),
//! )
//! .unwrap();
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
    rfc9610::CONTACTS_CAPABILITY,
};

/// Failure causes during a JMAP `ContactCard/changes` flow.
#[derive(Debug, Error)]
pub enum JmapContactCardChangesError {
    #[error("JMAP ContactCard/changes failed: {0}")]
    Changes(#[from] JmapChangesError),
}

/// Options for [`JmapContactCardChanges::new`].
#[derive(Clone, Debug, Default)]
pub struct JmapContactCardChangesOptions {
    /// Server-side cap on the number of changes returned.
    pub max_changes: Option<u64>,
}

/// I/O-free coroutine for the JMAP `ContactCard/changes` method.
pub struct JmapContactCardChanges {
    state: State,
}

impl JmapContactCardChanges {
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        since_state: impl Into<String>,
        opts: JmapContactCardChangesOptions,
    ) -> Result<Self, JmapContactCardChangesError> {
        let account_id = session
            .primary_accounts
            .get(CONTACTS_CAPABILITY)
            .cloned()
            .unwrap_or_default();
        let api_url = &session.api_url;

        Ok(Self {
            state: State::Changes(JmapChanges::new(
                account_id,
                http_auth,
                api_url,
                "ContactCard/changes",
                vec![CORE_CAPABILITY.into(), CONTACTS_CAPABILITY.into()],
                since_state,
                JmapChangesOptions {
                    max_changes: opts.max_changes,
                },
            )?),
        })
    }
}

impl JmapCoroutine for JmapContactCardChanges {
    type Yield = JmapYield;
    type Return = Result<JmapChangesOutput, JmapContactCardChangesError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> JmapCoroutineState<Self::Yield, Self::Return> {
        trace!("ContactCard/changes: {}", self.state);
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
