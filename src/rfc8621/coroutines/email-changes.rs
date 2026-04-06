//! I/O-free coroutine for `Email/changes` (RFC 8621 §4.3).

use alloc::{string::String, vec, vec::Vec};
use io_socket::io::{SocketInput, SocketOutput};
use secrecy::SecretString;
use thiserror::Error;

use crate::{
    rfc8620::coroutines::changes::{JmapChanges, JmapChangesError, JmapChangesResult},
    rfc8620::types::session::JmapSession,
    rfc8620::types::session::capabilities,
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
    Ok {
        new_state: String,
        has_more_changes: bool,
        created: Vec<String>,
        updated: Vec<String>,
        destroyed: Vec<String>,
        keep_alive: bool,
    },
    Io {
        input: SocketInput,
    },
    Err {
        err: JmapEmailChangesError,
    },
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
        Ok(Self {
            changes: JmapChanges::new(
                session,
                http_auth,
                "Email/changes",
                vec![capabilities::CORE.into(), capabilities::MAIL.into()],
                since_state,
                max_changes,
            )?,
        })
    }

    /// Makes the coroutine progress.
    pub fn resume(&mut self, arg: Option<SocketOutput>) -> JmapEmailChangesResult {
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
            JmapChangesResult::Io { input } => JmapEmailChangesResult::Io { input },
            JmapChangesResult::Err { err } => JmapEmailChangesResult::Err { err: err.into() },
        }
    }
}
