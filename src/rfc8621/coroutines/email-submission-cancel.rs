//! I/O-free coroutine for canceling pending `EmailSubmission` objects (RFC 8621 §7.5).

use std::collections::HashMap;

use io_stream::io::StreamIo;
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    rfc8620::coroutines::send::{JmapBatch, JmapSend, JmapSendError, JmapSendResult},
    rfc8620::types::session::JmapSession,
    rfc8620::types::{
        error::{JmapMethodError, SetError},
        session::capabilities,
    },
    rfc8621::types::email_submission::{EmailSubmission, EmailSubmissionUpdate, UndoStatus},
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

/// Result returned by the [`JmapEmailSubmissionCancel`] coroutine.
#[derive(Debug)]
pub enum JmapEmailSubmissionCancelResult {
    Ok {
        new_state: String,
        updated: HashMap<String, Option<EmailSubmission>>,
        not_updated: HashMap<String, SetError>,
        keep_alive: bool,
    },
    Io {
        io: StreamIo,
    },
    Err {
        err: JmapEmailSubmissionCancelError,
    },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EmailSubmissionCancelResponse {
    new_state: String,
    #[serde(default)]
    updated: Option<HashMap<String, Option<EmailSubmission>>>,
    #[serde(default)]
    not_updated: Option<HashMap<String, SetError>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CancelEmailSubmissionsArgs {
    account_id: String,
    update: HashMap<String, EmailSubmissionUpdate>,
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

    /// Makes the coroutine progress.
    pub fn resume(&mut self, arg: Option<StreamIo>) -> JmapEmailSubmissionCancelResult {
        let (response, keep_alive) = match self.send.resume(arg) {
            JmapSendResult::Ok {
                response,
                keep_alive,
            } => (response, keep_alive),
            JmapSendResult::Io { io } => return JmapEmailSubmissionCancelResult::Io { io },
            JmapSendResult::Err { err } => {
                return JmapEmailSubmissionCancelResult::Err { err: err.into() }
            }
        };

        let Some((name, args, _)) = response.method_responses.into_iter().next() else {
            return JmapEmailSubmissionCancelResult::Err {
                err: JmapEmailSubmissionCancelError::MissingResponse,
            };
        };

        if name == "error" {
            let err =
                serde_json::from_value::<JmapMethodError>(args).unwrap_or(JmapMethodError::Unknown);
            return JmapEmailSubmissionCancelResult::Err { err: err.into() };
        }

        match serde_json::from_value::<EmailSubmissionCancelResponse>(args) {
            Ok(r) => JmapEmailSubmissionCancelResult::Ok {
                new_state: r.new_state,
                updated: r.updated.unwrap_or_default(),
                not_updated: r.not_updated.unwrap_or_default(),
                keep_alive,
            },
            Err(err) => JmapEmailSubmissionCancelResult::Err {
                err: JmapEmailSubmissionCancelError::ParseResponse(err),
            },
        }
    }
}
