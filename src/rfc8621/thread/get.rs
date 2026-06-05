//! JMAP `Thread/get` coroutine (RFC 8621 §3.3): wraps the generic
//! [`JmapGet`] with the JMAP-Mail capability set.
//!
//! # Example
//!
//! ```rust,no_run
//! use io_jmap::{
//!     rfc8620::JmapSession,
//!     rfc8621::thread::get::JmapThreadGet,
//! };
//! use secrecy::SecretString;
//!
//! # fn demo(session: &JmapSession) {
//! let auth = SecretString::from("Bearer xyz");
//! let coroutine = JmapThreadGet::new(session, &auth, vec!["t1".into()]).unwrap();
//! # let _ = coroutine;
//! # }
//! ```

use core::fmt;

use alloc::{string::String, vec, vec::Vec};

use log::trace;
use secrecy::SecretString;
use thiserror::Error;

use crate::coroutine::*;
use crate::jmap_try;
use crate::{
    rfc8620::{CORE_CAPABILITY, JmapSession, get::*},
    rfc8621::{MAIL_CAPABILITY, thread::Thread},
};

/// Failure causes during a JMAP `Thread/get` flow.
#[derive(Debug, Error)]
pub enum JmapThreadGetError {
    #[error("JMAP Thread/get failed: {0}")]
    Get(#[from] JmapGetError),
}

/// Successful terminal output of [`JmapThreadGet`].
#[derive(Clone, Debug)]
pub struct JmapThreadGetOutput {
    pub threads: Vec<Thread>,
    pub not_found: Vec<String>,
    pub new_state: String,
    pub keep_alive: bool,
}

/// I/O-free coroutine for the JMAP `Thread/get` method.
pub struct JmapThreadGet {
    state: State,
}

impl JmapThreadGet {
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        ids: Vec<String>,
    ) -> Result<Self, JmapThreadGetError> {
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
                "Thread/get",
                vec![CORE_CAPABILITY.into(), MAIL_CAPABILITY.into()],
                Some(ids),
                None,
            )?),
        })
    }
}

impl JmapCoroutine for JmapThreadGet {
    type Yield = JmapYield;
    type Return = Result<JmapThreadGetOutput, JmapThreadGetError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> JmapCoroutineState<Self::Yield, Self::Return> {
        trace!("Thread/get: {}", self.state);
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
    Get(JmapGet<Thread>),
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Get(_) => f.write_str("get"),
        }
    }
}
