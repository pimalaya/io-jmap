//! JMAP `Identity/get` coroutine (RFC 8621 §6.3): wraps the generic
//! [`JmapGet`] with the Submission capability.
//!
//! # Example
//!
//! ```rust,no_run
//! use io_jmap::{
//!     rfc8620::JmapSession,
//!     rfc8621::identity::get::JmapIdentityGet,
//! };
//! use secrecy::SecretString;
//!
//! # fn demo(session: &JmapSession) {
//! let auth = SecretString::from("Bearer xyz");
//! let coroutine = JmapIdentityGet::new(session, &auth, None).unwrap();
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
    rfc8621::{MAIL_CAPABILITY, email_submission::SUBMISSION_CAPABILITY, identity::Identity},
};

/// Failure causes during a JMAP `Identity/get` flow.
#[derive(Debug, Error)]
pub enum JmapIdentityGetError {
    #[error("JMAP Identity/get failed: {0}")]
    Get(#[from] JmapGetError),
}

/// Successful terminal output of [`JmapIdentityGet`].
#[derive(Clone, Debug)]
pub struct JmapIdentityGetOutput {
    pub identities: Vec<Identity>,
    pub not_found: Vec<String>,
    pub new_state: String,
    pub keep_alive: bool,
}

/// I/O-free coroutine for the JMAP `Identity/get` method.
pub struct JmapIdentityGet {
    state: State,
}

impl JmapIdentityGet {
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        ids: Option<Vec<String>>,
    ) -> Result<Self, JmapIdentityGetError> {
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
                "Identity/get",
                vec![
                    CORE_CAPABILITY.into(),
                    MAIL_CAPABILITY.into(),
                    SUBMISSION_CAPABILITY.into(),
                ],
                ids,
                None,
            )?),
        })
    }
}

impl JmapCoroutine for JmapIdentityGet {
    type Yield = JmapYield;
    type Return = Result<JmapIdentityGetOutput, JmapIdentityGetError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> JmapCoroutineState<Self::Yield, Self::Return> {
        trace!("Identity/get: {}", self.state);
        match &mut self.state {
            State::Get(get) => {
                let JmapGetOutput {
                    list,
                    not_found,
                    state,
                    keep_alive,
                } = jmap_try!(get, arg);
                JmapCoroutineState::Complete(Ok(JmapIdentityGetOutput {
                    identities: list,
                    not_found,
                    new_state: state,
                    keep_alive,
                }))
            }
        }
    }
}

enum State {
    Get(JmapGet<Identity>),
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Get(_) => f.write_str("get"),
        }
    }
}
