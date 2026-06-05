//! JMAP `EmailSubmission/get` coroutine (RFC 8621 §7.2): wraps the
//! generic [`JmapGet`] with the Submission capability.
//!
//! # Example
//!
//! ```rust,no_run
//! use io_jmap::{
//!     rfc8620::JmapSession,
//!     rfc8621::email_submission::get::JmapEmailSubmissionGet,
//! };
//! use secrecy::SecretString;
//!
//! # fn demo(session: &JmapSession) {
//! let auth = SecretString::from("Bearer xyz");
//! let coroutine = JmapEmailSubmissionGet::new(session, &auth, None).unwrap();
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
    rfc8621::{
        MAIL_CAPABILITY,
        email_submission::{EmailSubmission, SUBMISSION_CAPABILITY},
    },
};

/// Failure causes during a JMAP `EmailSubmission/get` flow.
#[derive(Debug, Error)]
pub enum JmapEmailSubmissionGetError {
    #[error("JMAP EmailSubmission/get failed: {0}")]
    Get(#[from] JmapGetError),
}

/// Successful terminal output of [`JmapEmailSubmissionGet`].
#[derive(Clone, Debug)]
pub struct JmapEmailSubmissionGetOutput {
    pub submissions: Vec<EmailSubmission>,
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
        ids: Option<Vec<String>>,
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
                ids,
                None,
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
    Get(JmapGet<EmailSubmission>),
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Get(_) => f.write_str("get"),
        }
    }
}
