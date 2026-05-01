//! I/O-free coroutine for the `Thread/get` method (RFC 8621 §3.3).

use alloc::{string::String, vec, vec::Vec};

use secrecy::SecretString;
use thiserror::Error;

use crate::{
    rfc8620::get::{JmapGet, JmapGetError, JmapGetResult},
    rfc8620::session::JmapSession,
    rfc8621::capabilities,
    rfc8621::thread::Thread,
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
    /// The coroutine has successfully completed.
    Ok {
        threads: Vec<Thread>,
        not_found: Vec<String>,
        new_state: String,
        keep_alive: bool,
    },
    /// The coroutine needs more bytes to be read from the socket.
    WantsRead,
    /// The coroutine wants the given bytes to be written to the socket.
    WantsWrite(Vec<u8>),
    /// The coroutine encountered an error.
    Err(JmapThreadGetError),
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
        let account_id = session
            .primary_accounts
            .get(capabilities::MAIL)
            .cloned()
            .unwrap_or_default();
        let api_url = &session.api_url;

        Ok(Self {
            get: JmapGet::new(
                account_id,
                http_auth,
                api_url,
                "Thread/get",
                vec![capabilities::CORE.into(), capabilities::MAIL.into()],
                Some(ids),
                None,
            )?,
        })
    }

    /// Advances the coroutine.
    pub fn resume(&mut self, arg: Option<&[u8]>) -> JmapThreadGetResult {
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
            JmapGetResult::WantsRead => JmapThreadGetResult::WantsRead,
            JmapGetResult::WantsWrite(bytes) => JmapThreadGetResult::WantsWrite(bytes),
            JmapGetResult::Err(err) => JmapThreadGetResult::Err(err.into()),
        }
    }
}
