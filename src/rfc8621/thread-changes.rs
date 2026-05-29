//! I/O-free coroutine for `Thread/changes` (RFC 8621 §3.2).

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
pub enum JmapThreadChangesError {
    #[error("JMAP Thread/changes error: {0}")]
    Changes(#[from] JmapChangesError),
}

/// I/O-free coroutine for the JMAP `Thread/changes` method.
///
/// Returns the changes to threads since the given `since_state`.
pub struct JmapThreadChanges {
    changes: JmapChanges,
}

impl JmapThreadChanges {
    /// Creates a new coroutine.
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        since_state: impl Into<String>,
        max_changes: Option<u64>,
    ) -> Result<Self, JmapThreadChangesError> {
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
                "Thread/changes",
                vec![capabilities::CORE.into(), capabilities::MAIL.into()],
                since_state,
                max_changes,
            )?,
        })
    }
}

impl JmapCoroutine for JmapThreadChanges {
    type Yield = JmapYield;
    type Return = Result<JmapChangesOutput, JmapThreadChangesError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> JmapCoroutineState<Self::Yield, Self::Return> {
        match self.changes.resume(arg) {
            JmapCoroutineState::Complete(Ok(out)) => JmapCoroutineState::Complete(Ok(out)),
            JmapCoroutineState::Complete(Err(err)) => JmapCoroutineState::Complete(Err(err.into())),
            JmapCoroutineState::Yielded(y) => JmapCoroutineState::Yielded(y),
        }
    }
}
