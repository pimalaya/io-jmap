//! I/O-free coroutine for `Thread/changes` (RFC 8621 §3.2).

use io_stream::io::StreamIo;
use secrecy::SecretString;
use thiserror::Error;

use crate::{
    rfc8620::coroutines::changes::{JmapChanges, JmapChangesError, JmapChangesResult},
    rfc8620::types::session::capabilities,
    rfc8620::types::session::JmapSession,
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
    Ok {
        new_state: String,
        has_more_changes: bool,
        created: Vec<String>,
        updated: Vec<String>,
        destroyed: Vec<String>,
        keep_alive: bool,
    },
    Io {
        io: StreamIo,
    },
    Err {
        err: JmapThreadChangesError,
    },
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
        Ok(Self {
            changes: JmapChanges::new(
                session,
                http_auth,
                "Thread/changes",
                vec![capabilities::CORE.into(), capabilities::MAIL.into()],
                since_state,
                max_changes,
            )?,
        })
    }

    /// Makes the coroutine progress.
    pub fn resume(&mut self, arg: Option<StreamIo>) -> JmapThreadChangesResult {
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
            JmapChangesResult::Io { io } => JmapThreadChangesResult::Io { io },
            JmapChangesResult::Err { err } => JmapThreadChangesResult::Err { err: err.into() },
        }
    }
}
