//! I/O-free coroutine for the `EmailSubmission/get` method (RFC 8621 §7.2).

use io_stream::io::StreamIo;
use secrecy::SecretString;
use thiserror::Error;

use crate::{
    rfc8620::coroutines::get::{JmapGet, JmapGetError, JmapGetResult},
    rfc8620::types::session::capabilities,
    rfc8620::types::session::JmapSession,
    rfc8621::types::email_submission::EmailSubmission,
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
        io: StreamIo,
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
        Ok(Self {
            get: JmapGet::new(
                session,
                http_auth,
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

    pub fn resume(&mut self, arg: Option<StreamIo>) -> JmapEmailSubmissionGetResult {
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
            JmapGetResult::Io { io } => JmapEmailSubmissionGetResult::Io { io },
            JmapGetResult::Err { err } => JmapEmailSubmissionGetResult::Err { err: err.into() },
        }
    }
}
