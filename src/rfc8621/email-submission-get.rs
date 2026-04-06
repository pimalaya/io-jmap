//! I/O-free coroutine for the `EmailSubmission/get` method (RFC 8621 §7.2).

use alloc::{string::String, vec, vec::Vec};
use io_socket::io::{SocketInput, SocketOutput};
use secrecy::SecretString;
use thiserror::Error;

use crate::{
    rfc8620::get::{JmapGet, JmapGetError, JmapGetResult},
    rfc8620::session::JmapSession,
    rfc8621::capabilities,
    rfc8621::email_submission::EmailSubmission,
};

/// Errors that can occur during the coroutine progression.
#[derive(Debug, Error)]
pub enum JmapEmailSubmissionGetError {
    #[error("JMAP EmailSubmission/get error: {0}")]
    Get(#[from] JmapGetError),
}

/// Result returned by the [`JmapEmailSubmissionGet`] coroutine.
#[derive(Debug)]
pub enum JmapEmailSubmissionGetResult {
    Ok {
        submissions: Vec<EmailSubmission>,
        not_found: Vec<String>,
        new_state: String,
        keep_alive: bool,
    },
    Io {
        input: SocketInput,
    },
    Err {
        err: JmapEmailSubmissionGetError,
    },
}

/// I/O-free coroutine for the JMAP `EmailSubmission/get` method.
///
/// Fetches EmailSubmission objects by ID. Pass `ids: None` to fetch all.
pub struct JmapEmailSubmissionGet {
    get: JmapGet<EmailSubmission>,
}

impl JmapEmailSubmissionGet {
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        ids: Option<Vec<String>>,
    ) -> Result<Self, JmapEmailSubmissionGetError> {
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
                "EmailSubmission/get",
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

    pub fn resume(&mut self, arg: Option<SocketOutput>) -> JmapEmailSubmissionGetResult {
        match self.get.resume(arg) {
            JmapGetResult::Ok {
                list,
                not_found,
                state,
                keep_alive,
            } => JmapEmailSubmissionGetResult::Ok {
                submissions: list,
                not_found,
                new_state: state,
                keep_alive,
            },
            JmapGetResult::Io { input } => JmapEmailSubmissionGetResult::Io { input },
            JmapGetResult::Err { err } => JmapEmailSubmissionGetResult::Err { err: err.into() },
        }
    }
}
