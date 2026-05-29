//! I/O-free coroutine for `EmailSubmission/set` (RFC 8621 §7.5).

use alloc::{collections::BTreeMap, string::String, vec};

use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::coroutine::*;
use crate::{
    rfc8620::{error::JmapMethodError, send::*, session::JmapSession},
    rfc8621::{
        capabilities,
        email_submission::{EmailSubmission, EmailSubmissionCreate, EmailSubmissionSetError},
    },
};

/// Errors that can occur during the coroutine progression.
#[derive(Debug, Error)]
pub enum JmapEmailSubmissionSetError {
    #[error("Send JMAP request error: {0}")]
    Send(#[from] JmapSendError),
    #[error("Serialize EmailSubmission/set args error: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    #[error("Parse EmailSubmission/set response error: {0}")]
    ParseResponse(#[source] serde_json::Error),
    #[error("Missing EmailSubmission/set response in method_responses")]
    MissingResponse,
    #[error("JMAP EmailSubmission/set method error: {0}")]
    Method(#[from] JmapMethodError),
}

/// Successful terminal output of [`JmapEmailSubmissionSet`].
#[derive(Clone, Debug)]
pub struct JmapEmailSubmissionSetOutput {
    pub new_state: String,
    pub created: BTreeMap<String, EmailSubmission>,
    pub not_created: BTreeMap<String, EmailSubmissionSetError>,
    pub keep_alive: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EmailSubmissionSetResponse {
    new_state: String,
    #[serde(default)]
    created: Option<BTreeMap<String, EmailSubmission>>,
    #[serde(default)]
    not_created: Option<BTreeMap<String, EmailSubmissionSetError>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EmailSubmissionSetArgs {
    account_id: String,
    create: BTreeMap<String, EmailSubmissionCreate>,
}

/// I/O-free coroutine for the JMAP `EmailSubmission/set` method.
///
/// Submits emails for sending. This is the JMAP equivalent of SMTP
/// message submission.
pub struct JmapEmailSubmissionSet {
    send: JmapSend,
}

impl JmapEmailSubmissionSet {
    /// Creates a new coroutine.
    ///
    /// `submissions` is a map from client-assigned ID to
    /// [`EmailSubmissionCreate`].
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        submissions: BTreeMap<String, EmailSubmissionCreate>,
    ) -> Result<Self, JmapEmailSubmissionSetError> {
        let account_id = session
            .primary_accounts
            .get(capabilities::MAIL)
            .cloned()
            .unwrap_or_default();
        let api_url = &session.api_url;

        let args = serde_json::to_value(EmailSubmissionSetArgs {
            account_id,
            create: submissions,
        })
        .map_err(JmapEmailSubmissionSetError::SerializeArgs)?;

        let mut batch = JmapBatch::new();
        batch.add("EmailSubmission/set", args);
        let request = batch.into_request(vec![
            capabilities::CORE.into(),
            capabilities::MAIL.into(),
            capabilities::SUBMISSION.into(),
        ]);

        Ok(Self {
            send: JmapSend::new(http_auth, api_url, request)?,
        })
    }
}

impl JmapCoroutine for JmapEmailSubmissionSet {
    type Yield = JmapYield;
    type Return = Result<JmapEmailSubmissionSetOutput, JmapEmailSubmissionSetError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> JmapCoroutineState<Self::Yield, Self::Return> {
        let JmapSendOutput {
            response,
            keep_alive,
        } = match self.send.resume(arg) {
            JmapCoroutineState::Complete(Ok(out)) => out,
            JmapCoroutineState::Complete(Err(err)) => {
                return JmapCoroutineState::Complete(Err(err.into()));
            }
            JmapCoroutineState::Yielded(y) => return JmapCoroutineState::Yielded(y),
        };

        let Some((name, args, _)) = response.method_responses.into_iter().next() else {
            return JmapCoroutineState::Complete(Err(JmapEmailSubmissionSetError::MissingResponse));
        };

        if name == "error" {
            let err =
                serde_json::from_value::<JmapMethodError>(args).unwrap_or(JmapMethodError::Unknown);
            return JmapCoroutineState::Complete(Err(err.into()));
        }

        match serde_json::from_value::<EmailSubmissionSetResponse>(args) {
            Ok(r) => JmapCoroutineState::Complete(Ok(JmapEmailSubmissionSetOutput {
                new_state: r.new_state,
                created: r.created.unwrap_or_default(),
                not_created: r.not_created.unwrap_or_default(),
                keep_alive,
            })),
            Err(err) => {
                JmapCoroutineState::Complete(Err(JmapEmailSubmissionSetError::ParseResponse(err)))
            }
        }
    }
}
