//! JMAP `Thread/get` coroutine (RFC 8621 §3.3): wraps the generic [`JmapGet`]
//! with the JMAP-Mail capability set.
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
//!     rfc8620::session::JmapSession,
//!     rfc8621::thread::get::JmapThreadGet,
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
//! let mut coroutine = JmapThreadGet::new(&session, &auth, vec!["t1".into()]).unwrap();
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
//! println!("{} threads", out.threads.len());
//! ```

use alloc::{string::String, vec, vec::Vec};

use secrecy::SecretString;
use thiserror::Error;

use crate::{
    coroutine::*,
    jmap_try,
    rfc8620::{JMAP_CORE_CAPABILITY, get::*, session::JmapSession},
    rfc8621::{JMAP_MAIL_CAPABILITY, thread::JmapThread},
};

/// Failure causes during a JMAP `Thread/get` flow.
#[derive(Debug, Error)]
pub enum JmapThreadGetError {
    /// The inner generic get coroutine failed.
    #[error("JMAP Thread/get failed: {0}")]
    Get(#[from] JmapGetError),
}

/// Successful terminal output of [`JmapThreadGet`].
#[derive(Clone, Debug)]
pub struct JmapThreadGetOutput {
    /// The fetched threads.
    pub threads: Vec<JmapThread>,
    /// The requested ids the server did not find.
    pub not_found: Vec<String>,
    /// The new server state after the call.
    pub new_state: String,
    /// Whether the server indicated the connection can be reused.
    pub keep_alive: bool,
}

/// I/O-free coroutine for the JMAP `Thread/get` method.
pub struct JmapThreadGet {
    state: State,
}

impl JmapThreadGet {
    /// Prepares the method call request and builds the coroutine.
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        ids: Vec<String>,
    ) -> Result<Self, JmapThreadGetError> {
        let account_id = session
            .primary_accounts
            .get(JMAP_MAIL_CAPABILITY)
            .cloned()
            .unwrap_or_default();
        let api_url = &session.api_url;

        Ok(Self {
            state: State::Get(JmapGet::new(
                account_id,
                http_auth,
                api_url,
                "Thread/get",
                vec![JMAP_CORE_CAPABILITY.into(), JMAP_MAIL_CAPABILITY.into()],
                JmapGetOptions {
                    ids: Some(ids),
                    properties: None,
                },
            )?),
        })
    }
}

impl JmapCoroutine for JmapThreadGet {
    type Yield = JmapYield;
    type Return = Result<JmapThreadGetOutput, JmapThreadGetError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> JmapCoroutineState<Self::Yield, Self::Return> {
        match &mut self.state {
            State::Get(get) => {
                let JmapGetOutput {
                    list,
                    not_found,
                    state,
                    keep_alive,
                } = jmap_try!(get, arg);
                JmapCoroutineState::Complete(Ok(JmapThreadGetOutput {
                    threads: list,
                    not_found,
                    new_state: state,
                    keep_alive,
                }))
            }
        }
    }
}

enum State {
    Get(JmapGet<JmapThread>),
}
