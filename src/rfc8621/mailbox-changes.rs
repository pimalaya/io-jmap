//! I/O-free coroutine for `Mailbox/changes` (RFC 8621 §2.7).

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
pub enum JmapMailboxChangesError {
    #[error("JMAP Mailbox/changes error: {0}")]
    Changes(#[from] JmapChangesError),
}

/// Successful output of [`JmapMailboxChanges`].
#[derive(Clone, Debug)]
pub struct JmapMailboxChangesOk {
    pub new_state: String,
    pub has_more_changes: bool,
    pub created: Vec<String>,
    pub updated: Vec<String>,
    pub destroyed: Vec<String>,
    pub keep_alive: bool,
}

/// I/O-free coroutine for the JMAP `Mailbox/changes` method.
///
/// Returns the changes to mailboxes since the given `since_state`.
pub struct JmapMailboxChanges {
    changes: JmapChanges,
}

impl JmapMailboxChanges {
    /// Creates a new coroutine.
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        since_state: impl Into<String>,
        max_changes: Option<u64>,
    ) -> Result<Self, JmapMailboxChangesError> {
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
                "Mailbox/changes",
                vec![capabilities::CORE.into(), capabilities::MAIL.into()],
                since_state,
                max_changes,
            )?,
        })
    }
}

impl JmapCoroutine for JmapMailboxChanges {
    type Output = JmapMailboxChangesOk;
    type Error = JmapMailboxChangesError;

    fn resume(&mut self, arg: Option<&[u8]>) -> JmapCoroutineState<Self::Output, Self::Error> {
        match self.changes.resume(arg) {
            JmapCoroutineState::Done(JmapChangesOk {
                new_state,
                has_more_changes,
                created,
                updated,
                destroyed,
                keep_alive,
            }) => JmapCoroutineState::Done(JmapMailboxChangesOk {
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
