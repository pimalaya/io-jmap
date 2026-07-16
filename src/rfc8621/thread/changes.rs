//! JMAP `Thread/changes` coroutine (RFC 8621 §3.2): wraps the generic
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
//!     rfc8621::thread::changes::{JmapThreadChanges, JmapThreadChangesOptions},
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
//!     JmapThreadChanges::new(&session, &auth, "s1", JmapThreadChangesOptions::default())
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
    coroutine::*,
    jmap_try,
    rfc8620::{JMAP_CORE_CAPABILITY, JmapSession, changes::*},
    rfc8621::JMAP_MAIL_CAPABILITY,
};

/// Failure causes during a JMAP `Thread/changes` flow.
#[derive(Debug, Error)]
pub enum JmapThreadChangesError {
    /// The inner generic changes coroutine failed.
    #[error("JMAP Thread/changes failed: {0}")]
    Changes(#[from] JmapChangesError),
}

/// Options for [`JmapThreadChanges::new`].
#[derive(Clone, Debug, Default)]
pub struct JmapThreadChangesOptions {
    /// Server-side cap on the number of changes returned.
    pub max_changes: Option<u64>,
}

/// I/O-free coroutine for the JMAP `Thread/changes` method.
pub struct JmapThreadChanges {
    state: State,
}

impl JmapThreadChanges {
    /// Prepares the method call request and builds the coroutine.
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        since_state: impl Into<String>,
        opts: JmapThreadChangesOptions,
    ) -> Result<Self, JmapThreadChangesError> {
        let account_id = session
            .primary_accounts
            .get(JMAP_MAIL_CAPABILITY)
            .cloned()
            .unwrap_or_default();
        let api_url = &session.api_url;

        Ok(Self {
            state: State::Changes(JmapChanges::new(
                account_id,
                http_auth,
                api_url,
                "Thread/changes",
                vec![JMAP_CORE_CAPABILITY.into(), JMAP_MAIL_CAPABILITY.into()],
                since_state,
                JmapChangesOptions {
                    max_changes: opts.max_changes,
                },
            )?),
        })
    }
}

impl JmapCoroutine for JmapThreadChanges {
    type Yield = JmapYield;
    type Return = Result<JmapChangesOutput, JmapThreadChangesError>;

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
