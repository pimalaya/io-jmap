//! I/O-free coroutine for canceling pending `EmailSubmission` objects (RFC 8621 §7.5).

use alloc::{collections::BTreeMap, string::String, vec, vec::Vec};

use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::coroutine::*;
use crate::{
    rfc8620::{error::JmapMethodError, send::*, session::JmapSession},
    rfc8621::{
        capabilities,
        email_submission::{
            EmailSubmission, EmailSubmissionSetError, EmailSubmissionUpdate, UndoStatus,
        },
    },
};

/// Errors that can occur during the coroutine progression.
#[derive(Debug, Error)]
pub enum JmapEmailSubmissionCancelError {
    #[error("Send JMAP request error: {0}")]
    Send(#[from] JmapSendError),
    #[error("Serialize EmailSubmission/set (cancel) args error: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    #[error("Parse EmailSubmission/set (cancel) response error: {0}")]
    ParseResponse(#[source] serde_json::Error),
    #[error("Missing EmailSubmission/set (cancel) response in method_responses")]
    MissingResponse,
    #[error("JMAP EmailSubmission/set (cancel) method error: {0}")]
    Method(#[from] JmapMethodError),
}

/// Successful terminal output of [`JmapEmailSubmissionCancel`].
#[derive(Clone, Debug)]
pub struct JmapEmailSubmissionCancelOutput {
    pub new_state: String,
    pub updated: BTreeMap<String, Option<EmailSubmission>>,
    pub not_updated: BTreeMap<String, EmailSubmissionSetError>,
    pub keep_alive: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EmailSubmissionCancelResponse {
    new_state: String,
    #[serde(default)]
    updated: Option<BTreeMap<String, Option<EmailSubmission>>>,
    #[serde(default)]
    not_updated: Option<BTreeMap<String, EmailSubmissionSetError>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CancelEmailSubmissionsArgs {
    account_id: String,
    update: BTreeMap<String, EmailSubmissionUpdate>,
}

/// I/O-free coroutine for canceling pending JMAP email submissions.
///
/// Issues an `EmailSubmission/set` request with `undoStatus: "canceled"` for
/// each of the given submission IDs. Only submissions with
/// `undoStatus: "pending"` can be canceled; the server will report the others
/// in `notUpdated`.
pub struct JmapEmailSubmissionCancel {
    send: JmapSend,
}

impl JmapEmailSubmissionCancel {
    /// Creates a new coroutine.
    ///
    /// `ids` is the list of submission IDs to cancel.
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        ids: Vec<String>,
    ) -> Result<Self, JmapEmailSubmissionCancelError> {
        let account_id = session
            .primary_accounts
            .get(capabilities::MAIL)
            .cloned()
            .unwrap_or_default();
        let api_url = &session.api_url;

        let update = ids
            .into_iter()
            .map(|id| {
                (
                    id,
                    EmailSubmissionUpdate {
                        undo_status: Some(UndoStatus::Canceled),
                    },
                )
            })
            .collect();

        let args = serde_json::to_value(CancelEmailSubmissionsArgs { account_id, update })
            .map_err(JmapEmailSubmissionCancelError::SerializeArgs)?;

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

impl JmapCoroutine for JmapEmailSubmissionCancel {
    type Yield = JmapYield;
    type Return = Result<JmapEmailSubmissionCancelOutput, JmapEmailSubmissionCancelError>;

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
            return JmapCoroutineState::Complete(Err(
                JmapEmailSubmissionCancelError::MissingResponse,
            ));
        };

        if name == "error" {
            let err =
                serde_json::from_value::<JmapMethodError>(args).unwrap_or(JmapMethodError::Unknown);
            return JmapCoroutineState::Complete(Err(err.into()));
        }

        match serde_json::from_value::<EmailSubmissionCancelResponse>(args) {
            Ok(r) => JmapCoroutineState::Complete(Ok(JmapEmailSubmissionCancelOutput {
                new_state: r.new_state,
                updated: r.updated.unwrap_or_default(),
                not_updated: r.not_updated.unwrap_or_default(),
                keep_alive,
            })),
            Err(err) => JmapCoroutineState::Complete(Err(
                JmapEmailSubmissionCancelError::ParseResponse(err),
            )),
        }
    }
}
