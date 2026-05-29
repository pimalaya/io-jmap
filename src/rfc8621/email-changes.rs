//! I/O-free coroutine for `Email/changes` (RFC 8621 §4.3).

use alloc::{string::String, vec, vec::Vec};

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

/// Successful output of [`JmapEmailChanges`].
#[derive(Clone, Debug)]
pub struct JmapEmailChangesOk {
    pub new_state: String,
    pub has_more_changes: bool,
    pub created: Vec<String>,
    pub updated: Vec<String>,
    pub destroyed: Vec<String>,
    pub keep_alive: bool,
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
    type Output = JmapEmailChangesOk;
    type Error = JmapEmailChangesError;

    fn resume(&mut self, arg: Option<&[u8]>) -> JmapCoroutineState<Self::Output, Self::Error> {
        match self.changes.resume(arg) {
            JmapCoroutineState::Done(JmapChangesOk {
                new_state,
                has_more_changes,
                created,
                updated,
                destroyed,
                keep_alive,
            }) => JmapCoroutineState::Done(JmapEmailChangesOk {
                new_state,
                has_more_changes,
                created,
                updated,
                destroyed,
                keep_alive,
            }),
            JmapCoroutineState::WantsRead => JmapCoroutineState::WantsRead,
            JmapCoroutineState::WantsWrite(bytes) => JmapCoroutineState::WantsWrite(bytes),
            JmapCoroutineState::Err(err) => JmapCoroutineState::Err(err.into()),
        }
    }
}
