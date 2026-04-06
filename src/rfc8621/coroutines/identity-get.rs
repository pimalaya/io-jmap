//! I/O-free coroutine for the `Identity/get` method (RFC 8621 §6.3).

use alloc::{string::String, vec, vec::Vec};
use io_socket::io::{SocketInput, SocketOutput};
use secrecy::SecretString;
use thiserror::Error;

use crate::{
    rfc8620::coroutines::get::{JmapGet, JmapGetError, JmapGetResult},
    rfc8620::types::session::JmapSession,
    rfc8620::types::session::capabilities,
    rfc8621::types::identity::Identity,
};

/// Errors that can occur during the coroutine progression.
#[derive(Debug, Error)]
pub enum JmapIdentityGetError {
    #[error("JMAP Identity/get error: {0}")]
    Get(#[from] JmapGetError),
}

/// Result returned by the [`JmapIdentityGet`] coroutine.
#[derive(Debug)]
pub enum JmapIdentityGetResult {
    Ok {
        identities: Vec<Identity>,
        not_found: Vec<String>,
        new_state: String,
        keep_alive: bool,
    },
    Io {
        input: SocketInput,
    },
    Err {
        err: JmapIdentityGetError,
    },
}

/// I/O-free coroutine for the JMAP `Identity/get` method.
///
/// Fetches sender identity objects. Pass `ids: None` to fetch all identities.
pub struct JmapIdentityGet {
    get: JmapGet<Identity>,
}

impl JmapIdentityGet {
    /// Creates a new coroutine.
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        ids: Option<Vec<String>>,
    ) -> Result<Self, JmapIdentityGetError> {
        Ok(Self {
            get: JmapGet::new(
                session,
                http_auth,
                "Identity/get",
                vec![
                    capabilities::CORE.into(),
                    capabilities::MAIL.into(),
                    capabilities::SUBMISSION.into(),
                ],
                ids,
                None,
            )?,
        })
    }

    /// Makes the coroutine progress.
    pub fn resume(&mut self, arg: Option<SocketOutput>) -> JmapIdentityGetResult {
        match self.get.resume(arg) {
            JmapGetResult::Ok {
                list,
                not_found,
                state,
                keep_alive,
            } => JmapIdentityGetResult::Ok {
                identities: list,
                not_found,
                new_state: state,
                keep_alive,
            },
            JmapGetResult::Io { input } => JmapIdentityGetResult::Io { input },
            JmapGetResult::Err { err } => JmapIdentityGetResult::Err { err: err.into() },
        }
    }
}
