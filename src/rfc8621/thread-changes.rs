//! I/O-free coroutine for `Thread/changes` (RFC 8621 §3.2).

use alloc::{string::String, vec, vec::Vec};

use secrecy::SecretString;
use thiserror::Error;

use crate::{
    rfc8620::changes::{JmapChanges, JmapChangesError, JmapChangesResult},
    rfc8620::session::JmapSession,
    rfc8621::capabilities,
};

/// Errors that can occur during the coroutine progression.
#[derive(Debug, Error)]
pub enum JmapThreadChangesError {
    #[error("JMAP Thread/changes error: {0}")]
    Changes(#[from] JmapChangesError),
}

/// Result returned by the [`JmapThreadChanges`] coroutine.
#[derive(Debug)]
pub enum JmapThreadChangesResult {
    /// The coroutine has successfully completed.
    Ok {
        new_state: String,
        has_more_changes: bool,
        created: Vec<String>,
        updated: Vec<String>,
        destroyed: Vec<String>,
        keep_alive: bool,
    },
    /// The coroutine needs more bytes to be read from the socket.
    WantsRead,
    /// The coroutine wants the given bytes to be written to the socket.
    WantsWrite(Vec<u8>),
    /// The coroutine encountered an error.
    Err(JmapThreadChangesError),
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

    /// Advances the coroutine.
    pub fn resume(&mut self, arg: Option<&[u8]>) -> JmapThreadChangesResult {
        match self.changes.resume(arg) {
            JmapChangesResult::Ok {
                new_state,
                has_more_changes,
                created,
                updated,
                destroyed,
                keep_alive,
            } => JmapThreadChangesResult::Ok {
                new_state,
                has_more_changes,
                created,
                updated,
                destroyed,
                keep_alive,
            },
            JmapChangesResult::WantsRead => JmapThreadChangesResult::WantsRead,
            JmapChangesResult::WantsWrite(bytes) => JmapThreadChangesResult::WantsWrite(bytes),
            JmapChangesResult::Err(err) => JmapThreadChangesResult::Err(err.into()),
        }
    }
}
