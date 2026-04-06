//! I/O-free coroutine for `Mailbox/changes` (RFC 8621 §2.7).

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
pub enum JmapMailboxChangesError {
    #[error("JMAP Mailbox/changes error: {0}")]
    Changes(#[from] JmapChangesError),
}

/// Result returned by the [`JmapMailboxChanges`] coroutine.
#[derive(Debug)]
pub enum JmapMailboxChangesResult {
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
        err: JmapMailboxChangesError,
    },
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
        Ok(Self {
            changes: JmapChanges::new(
                session,
                http_auth,
                "Mailbox/changes",
                vec![capabilities::CORE.into(), capabilities::MAIL.into()],
                since_state,
                max_changes,
            )?,
        })
    }

    /// Makes the coroutine progress.
    pub fn resume(&mut self, arg: Option<SocketOutput>) -> JmapMailboxChangesResult {
        match self.changes.resume(arg) {
            JmapChangesResult::Ok {
                new_state,
                has_more_changes,
                created,
                updated,
                destroyed,
                keep_alive,
            } => JmapMailboxChangesResult::Ok {
                new_state,
                has_more_changes,
                created,
                updated,
                destroyed,
                keep_alive,
            },
            JmapChangesResult::Io { input } => JmapMailboxChangesResult::Io { input },
            JmapChangesResult::Err { err } => JmapMailboxChangesResult::Err { err: err.into() },
        }
    }
}
