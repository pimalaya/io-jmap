//! I/O-free coroutine for the `Identity/get` method (RFC 8621 §6.3).

use alloc::{string::String, vec, vec::Vec};
use io_socket::io::{SocketInput, SocketOutput};
use secrecy::SecretString;
use thiserror::Error;

use crate::{
    rfc8620::get::{JmapGet, JmapGetError, JmapGetResult},
    rfc8620::session::JmapSession,
    rfc8621::capabilities,
    rfc8621::identity::Identity,
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
