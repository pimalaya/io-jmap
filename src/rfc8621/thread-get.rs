//! I/O-free coroutine for the `Thread/get` method (RFC 8621 §3.3).

use alloc::{string::String, vec, vec::Vec};

use secrecy::SecretString;
use thiserror::Error;

use crate::coroutine::*;
use crate::{
    rfc8620::{get::*, session::JmapSession},
    rfc8621::{capabilities, thread::Thread},
};

/// Errors that can occur during the coroutine progression.
#[derive(Debug, Error)]
pub enum JmapThreadGetError {
    #[error("JMAP Thread/get error: {0}")]
    Get(#[from] JmapGetError),
}

/// Successful output of [`JmapThreadGet`].
#[derive(Clone, Debug)]
pub struct JmapThreadGetOk {
    pub threads: Vec<Thread>,
    pub not_found: Vec<String>,
    pub new_state: String,
    pub keep_alive: bool,
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
}

impl JmapCoroutine for JmapThreadGet {
    type Output = JmapThreadGetOk;
    type Error = JmapThreadGetError;

    fn resume(&mut self, arg: Option<&[u8]>) -> JmapCoroutineState<Self::Output, Self::Error> {
        match self.get.resume(arg) {
            JmapGetResult::Ok {
                list,
                not_found,
                state,
                keep_alive,
            } => JmapCoroutineState::Done(JmapThreadGetOk {
                threads: list,
                not_found,
                new_state: state,
                keep_alive,
            }),
            JmapGetResult::WantsRead => JmapCoroutineState::WantsRead,
            JmapGetResult::WantsWrite(bytes) => JmapCoroutineState::WantsWrite(bytes),
            JmapGetResult::Err(err) => JmapCoroutineState::Err(err.into()),
        }
    }
}

/// Output of the [`JmapClientStd::thread_get`] client method.
///
/// [`JmapClientStd::thread_get`]: crate::client::JmapClientStd::thread_get
#[derive(Clone, Debug)]
pub struct JmapThreadGetOutput {
    pub threads: Vec<Thread>,
    pub not_found: Vec<String>,
    pub new_state: String,
}
