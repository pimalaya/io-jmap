//! I/O-free coroutine for the `Thread/get` method (RFC 8621 §3.3).

use io_stream::io::StreamIo;
use secrecy::SecretString;
use thiserror::Error;

use crate::{
    rfc8620::coroutines::get::{JmapGet, JmapGetError, JmapGetResult},
    rfc8620::types::session::capabilities,
    rfc8620::types::session::JmapSession,
    rfc8621::types::thread::Thread,
};

/// Errors that can occur during the coroutine progression.
#[derive(Debug, Error)]
pub enum JmapThreadGetError {
    #[error("JMAP Thread/get error: {0}")]
    Get(#[from] JmapGetError),
}

/// Result returned by the [`JmapThreadGet`] coroutine.
#[derive(Debug)]
pub enum JmapThreadGetResult {
    Ok {
        threads: Vec<Thread>,
        not_found: Vec<String>,
        new_state: String,
        keep_alive: bool,
    },
    Io {
        io: StreamIo,
    },
    Err {
        err: JmapThreadGetError,
    },
}

/// I/O-free coroutine for the JMAP `Thread/get` method.
///
/// Fetches thread objects by ID, each containing an ordered list of
/// email IDs in the thread.
pub struct JmapThreadGet {
    get: JmapGet<Thread>,
}

impl JmapThreadGet {
    /// Creates a new coroutine.
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        ids: Vec<String>,
    ) -> Result<Self, JmapThreadGetError> {
        Ok(Self {
            get: JmapGet::new(
                session,
                http_auth,
                "Thread/get",
                vec![capabilities::CORE.into(), capabilities::MAIL.into()],
                Some(ids),
                None,
            )?,
        })
    }

    /// Makes the coroutine progress.
    pub fn resume(&mut self, arg: Option<StreamIo>) -> JmapThreadGetResult {
        match self.get.resume(arg) {
            JmapGetResult::Ok {
                list,
                not_found,
                state,
                keep_alive,
            } => JmapThreadGetResult::Ok {
                threads: list,
                not_found,
                new_state: state,
                keep_alive,
            },
            JmapGetResult::Io { io } => JmapThreadGetResult::Io { io },
            JmapGetResult::Err { err } => JmapThreadGetResult::Err { err: err.into() },
        }
    }
}
