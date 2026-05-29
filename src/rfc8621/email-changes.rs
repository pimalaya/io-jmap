//! I/O-free coroutine for `Email/changes` (RFC 8621 §4.3).

use alloc::{string::String, vec};

use secrecy::SecretString;
use thiserror::Error;

use crate::coroutine::*;
use crate::{
    rfc8620::{changes::*, session::JmapSession},
    rfc8621::capabilities,
};

/// Errors that can occur during the coroutine progression.
#[derive(Debug, Error)]
pub enum JmapEmailChangesError {
    #[error("JMAP Email/changes error: {0}")]
    Changes(#[from] JmapChangesError),
}

/// I/O-free coroutine for the JMAP `Email/changes` method.
///
/// Returns the changes to emails since the given `since_state`.
pub struct JmapEmailChanges {
    changes: JmapChanges,
}

impl JmapEmailChanges {
    /// Creates a new coroutine.
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        since_state: impl Into<String>,
        max_changes: Option<u64>,
    ) -> Result<Self, JmapEmailChangesError> {
        let account_id = session
            .primary_accounts
            .get(capabilities::MAIL)
            .cloned()
            .unwrap_or_default();
        let api_url = &session.api_url;

        Ok(Self {
            changes: JmapChanges::new(
                account_id,
                http_auth,
                api_url,
                "Email/changes",
                vec![capabilities::CORE.into(), capabilities::MAIL.into()],
                since_state,
                max_changes,
            )?,
        })
    }
}

impl JmapCoroutine for JmapEmailChanges {
    type Yield = JmapYield;
    type Return = Result<JmapChangesOutput, JmapEmailChangesError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> JmapCoroutineState<Self::Yield, Self::Return> {
        match self.changes.resume(arg) {
            JmapCoroutineState::Complete(Ok(out)) => JmapCoroutineState::Complete(Ok(out)),
            JmapCoroutineState::Complete(Err(err)) => JmapCoroutineState::Complete(Err(err.into())),
            JmapCoroutineState::Yielded(y) => JmapCoroutineState::Yielded(y),
        }
    }
}
