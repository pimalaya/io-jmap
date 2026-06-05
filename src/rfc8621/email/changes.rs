//! JMAP `Email/changes` coroutine (RFC 8621 §4.3): wraps the generic
//! [`JmapChanges`] with the JMAP-Mail capability set.
//!
//! # Example
//!
//! ```rust,no_run
//! use io_jmap::{
//!     rfc8620::JmapSession,
//!     rfc8621::email::changes::JmapEmailChanges,
//! };
//! use secrecy::SecretString;
//!
//! # fn demo(session: &JmapSession) {
//! let auth = SecretString::from("Bearer xyz");
//! let coroutine = JmapEmailChanges::new(session, &auth, "s1", None).unwrap();
//! # let _ = coroutine;
//! # }
//! ```

use core::fmt;

use alloc::{string::String, vec};

use log::trace;
use secrecy::SecretString;
use thiserror::Error;

use crate::coroutine::*;
use crate::jmap_try;
use crate::{
    rfc8620::{CORE_CAPABILITY, JmapSession, changes::*},
    rfc8621::MAIL_CAPABILITY,
};

/// Failure causes during a JMAP `Email/changes` flow.
#[derive(Debug, Error)]
pub enum JmapEmailChangesError {
    #[error("JMAP Email/changes failed: {0}")]
    Changes(#[from] JmapChangesError),
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
        max_changes: Option<u64>,
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
                max_changes,
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
