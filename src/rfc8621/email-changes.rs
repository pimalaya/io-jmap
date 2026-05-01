//! I/O-free coroutine for `Email/changes` (RFC 8621 §4.3).

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
pub enum JmapEmailChangesError {
    #[error("JMAP Email/changes error: {0}")]
    Changes(#[from] JmapChangesError),
}

/// Result returned by the [`JmapEmailChanges`] coroutine.
#[derive(Debug)]
pub enum JmapEmailChangesResult {
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
    Err(JmapEmailChangesError),
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

    /// Advances the coroutine.
    pub fn resume(&mut self, arg: Option<&[u8]>) -> JmapEmailChangesResult {
        match self.changes.resume(arg) {
            JmapChangesResult::Ok {
                new_state,
                has_more_changes,
                created,
                updated,
                destroyed,
                keep_alive,
            } => JmapEmailChangesResult::Ok {
                new_state,
                has_more_changes,
                created,
                updated,
                destroyed,
                keep_alive,
            },
            JmapChangesResult::WantsRead => JmapEmailChangesResult::WantsRead,
            JmapChangesResult::WantsWrite(bytes) => JmapEmailChangesResult::WantsWrite(bytes),
            JmapChangesResult::Err(err) => JmapEmailChangesResult::Err(err.into()),
        }
    }
}
