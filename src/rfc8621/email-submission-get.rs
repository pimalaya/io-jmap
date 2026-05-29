//! I/O-free coroutine for the `EmailSubmission/get` method (RFC 8621 §7.2).

use alloc::{string::String, vec, vec::Vec};

use secrecy::SecretString;
use thiserror::Error;

use crate::coroutine::*;
use crate::{
    rfc8620::{get::*, session::JmapSession},
    rfc8621::{capabilities, email_submission::EmailSubmission},
};

/// Errors that can occur during the coroutine progression.
#[derive(Debug, Error)]
pub enum JmapEmailSubmissionGetError {
    #[error("JMAP EmailSubmission/get error: {0}")]
    Get(#[from] JmapGetError),
}

/// Successful output of [`JmapEmailSubmissionGet`].
#[derive(Clone, Debug)]
pub struct JmapEmailSubmissionGetOk {
    pub submissions: Vec<EmailSubmission>,
    pub not_found: Vec<String>,
    pub new_state: String,
    pub keep_alive: bool,
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
}

impl JmapCoroutine for JmapEmailSubmissionGet {
    type Output = JmapEmailSubmissionGetOk;
    type Error = JmapEmailSubmissionGetError;

    fn resume(&mut self, arg: Option<&[u8]>) -> JmapCoroutineState<Self::Output, Self::Error> {
        match self.get.resume(arg) {
            JmapGetResult::Ok {
                list,
                not_found,
                state,
                keep_alive,
            } => JmapCoroutineState::Done(JmapEmailSubmissionGetOk {
                submissions: list,
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

/// Output of the [`JmapClientStd::email_submission_get`] client method.
///
/// [`JmapClientStd::email_submission_get`]: crate::client::JmapClientStd::email_submission_get
#[derive(Clone, Debug)]
pub struct JmapEmailSubmissionGetOutput {
    pub submissions: Vec<EmailSubmission>,
    pub not_found: Vec<String>,
    pub new_state: String,
}
