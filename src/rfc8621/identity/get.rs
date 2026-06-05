//! JMAP `Identity/get` coroutine (RFC 8621 §6.3): wraps the generic [`JmapGet`]
//! with the Submission capability.
//!
//! # Example
//!
//! ```rust,no_run
//! use io_jmap::{
//!     rfc8620::JmapSession,
//!     rfc8621::identity::get::{JmapIdentityGet, JmapIdentityGetOptions},
//! };
//! use secrecy::SecretString;
//!
//! # fn demo(session: &JmapSession) {
//! let auth = SecretString::from("Bearer xyz");
//! let coroutine =
//!     JmapIdentityGet::new(session, &auth, JmapIdentityGetOptions::default()).unwrap();
//! # let _ = coroutine;
//! # }
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
    rfc8621::{MAIL_CAPABILITY, email_submission::SUBMISSION_CAPABILITY, identity::JmapIdentity},
};

/// Failure causes during a JMAP `Identity/get` flow.
#[derive(Debug, Error)]
pub enum JmapIdentityGetError {
    #[error("JMAP Identity/get failed: {0}")]
    Get(#[from] JmapGetError),
}

/// Options for [`JmapIdentityGet::new`].
#[derive(Clone, Debug, Default)]
pub struct JmapIdentityGetOptions {
    /// Restrict the fetch to these identity IDs; `None` fetches all.
    pub ids: Option<Vec<String>>,
}

/// Successful terminal output of [`JmapIdentityGet`].
#[derive(Clone, Debug)]
pub struct JmapIdentityGetOutput {
    pub identities: Vec<JmapIdentity>,
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
        opts: JmapIdentityGetOptions,
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
                JmapGetOptions {
                    ids: opts.ids,
                    properties: None,
                },
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
    Get(JmapGet<JmapIdentity>),
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Get(_) => f.write_str("get"),
        }
    }
}
