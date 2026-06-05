//! JMAP `EmailSubmission/get` coroutine (RFC 8621 §7.2): wraps the
//! generic [`JmapGet`] with the Submission capability.
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
//!     rfc8621::email_submission::get::{
//!         JmapEmailSubmissionGet, JmapEmailSubmissionGetOptions,
//!     },
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
//! let mut coroutine = JmapEmailSubmissionGet::new(
//!     &session,
//!     &auth,
//!     JmapEmailSubmissionGetOptions::default(),
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
//! println!("{} submissions", out.submissions.len());
//! ```

use core::fmt;

use alloc::{string::String, vec, vec::Vec};

use log::trace;
use secrecy::SecretString;
use thiserror::Error;

use crate::{
    coroutine::*,
    jmap_try,
    rfc8620::{CORE_CAPABILITY, JmapSession, get::*},
    rfc8621::{
        MAIL_CAPABILITY,
        email_submission::{JmapEmailSubmission, SUBMISSION_CAPABILITY},
    },
};

/// Failure causes during a JMAP `EmailSubmission/get` flow.
#[derive(Debug, Error)]
pub enum JmapEmailSubmissionGetError {
    #[error("JMAP EmailSubmission/get failed: {0}")]
    Get(#[from] JmapGetError),
}

/// Options for [`JmapEmailSubmissionGet::new`].
#[derive(Clone, Debug, Default)]
pub struct JmapEmailSubmissionGetOptions {
    /// Restrict the fetch to these submission IDs; `None` fetches all.
    pub ids: Option<Vec<String>>,
}

/// Successful terminal output of [`JmapEmailSubmissionGet`].
#[derive(Clone, Debug)]
pub struct JmapEmailSubmissionGetOutput {
    pub submissions: Vec<JmapEmailSubmission>,
    pub not_found: Vec<String>,
    pub new_state: String,
    pub keep_alive: bool,
}

/// I/O-free coroutine for the JMAP `EmailSubmission/get` method.
pub struct JmapEmailSubmissionGet {
    state: State,
}

impl JmapEmailSubmissionGet {
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        opts: JmapEmailSubmissionGetOptions,
    ) -> Result<Self, JmapEmailSubmissionGetError> {
        let account_id = session
            .primary_accounts
            .get(MAIL_CAPABILITY)
            .cloned()
            .unwrap_or_default();
        let api_url = &session.api_url;

        Ok(Self {
            state: State::Get(JmapGet::new(
                account_id,
                http_auth,
                api_url,
                "EmailSubmission/get",
                vec![
                    CORE_CAPABILITY.into(),
                    MAIL_CAPABILITY.into(),
                    SUBMISSION_CAPABILITY.into(),
                ],
                JmapGetOptions {
                    ids: opts.ids,
                    properties: None,
                },
            )?),
        })
    }
}

impl JmapCoroutine for JmapEmailSubmissionGet {
    type Yield = JmapYield;
    type Return = Result<JmapEmailSubmissionGetOutput, JmapEmailSubmissionGetError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> JmapCoroutineState<Self::Yield, Self::Return> {
        trace!("EmailSubmission/get: {}", self.state);
        match &mut self.state {
            State::Get(get) => {
                let JmapGetOutput {
                    list,
                    not_found,
                    state,
                    keep_alive,
                } = jmap_try!(get, arg);
                JmapCoroutineState::Complete(Ok(JmapEmailSubmissionGetOutput {
                    submissions: list,
                    not_found,
                    new_state: state,
                    keep_alive,
                }))
            }
        }
    }
}

enum State {
    Get(JmapGet<JmapEmailSubmission>),
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Get(_) => f.write_str("get"),
        }
    }
}
