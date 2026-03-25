//! I/O-free coroutine for canceling pending `EmailSubmission` objects (RFC 8621 §7.5).

use std::collections::HashMap;

use io_stream::io::StreamIo;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    context::JmapContext,
    coroutines::send::{JmapBatch, SendJmapRequest, SendJmapRequestError, SendJmapRequestResult},
    types::{
        email_submission::{EmailSubmission, EmailSubmissionUpdate, UndoStatus},
        error::JmapMethodError,
        session::capabilities,
    },
};

/// Errors that can occur during the coroutine progression.
#[derive(Debug, Error)]
pub enum CancelJmapEmailSubmissionsError {
    #[error("Send JMAP request error: {0}")]
    Send(#[from] SendJmapRequestError),
    #[error("Serialize EmailSubmission/set (cancel) args error: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    #[error("Parse EmailSubmission/set (cancel) response error: {0}")]
    ParseResponse(#[source] serde_json::Error),
    #[error("Missing EmailSubmission/set (cancel) response in method_responses")]
    MissingResponse,
    #[error("JMAP EmailSubmission/set (cancel) method error: {0}")]
    Method(#[from] JmapMethodError),
}

/// Per-object set error.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetError {
    #[serde(rename = "type")]
    pub error_type: String,
    pub description: Option<String>,
    #[serde(default)]
    pub properties: Vec<String>,
}

/// Result returned by the [`CancelJmapEmailSubmissions`] coroutine.
#[derive(Debug)]
pub enum CancelJmapEmailSubmissionsResult {
    Ok {
        context: JmapContext,
        new_state: String,
        updated: HashMap<String, Option<EmailSubmission>>,
        not_updated: HashMap<String, SetError>,
        keep_alive: bool,
    },
    Io(StreamIo),
    Err {
        context: JmapContext,
        err: CancelJmapEmailSubmissionsError,
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
pub struct CancelJmapEmailSubmissions {
    send: SendJmapRequest,
}

impl CancelJmapEmailSubmissions {
    /// Creates a new coroutine.
    ///
    /// `ids` is the list of submission IDs to cancel.
    pub fn new(
        context: JmapContext,
        ids: Vec<String>,
    ) -> Result<Self, CancelJmapEmailSubmissionsError> {
        let account_id = context.account_id.clone().unwrap_or_default();
        let api_url = context
            .api_url()
            .cloned()
            .unwrap_or_else(|| "http://localhost".parse().unwrap());

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
            .map_err(CancelJmapEmailSubmissionsError::SerializeArgs)?;

        let mut batch = JmapBatch::new();
        batch.add("EmailSubmission/set", args);
        let request = batch.into_request(vec![
            capabilities::CORE.into(),
            capabilities::MAIL.into(),
            capabilities::SUBMISSION.into(),
        ]);

        Ok(Self {
            send: SendJmapRequest::new(context, &api_url, request)?,
        })
    }

    /// Makes the coroutine progress.
    pub fn resume(&mut self, arg: Option<StreamIo>) -> CancelJmapEmailSubmissionsResult {
        let (context, response, keep_alive) = match self.send.resume(arg) {
            SendJmapRequestResult::Ok { context, response, keep_alive } => {
                (context, response, keep_alive)
            }
            SendJmapRequestResult::Io(io) => return CancelJmapEmailSubmissionsResult::Io(io),
            SendJmapRequestResult::Err { context, err } => {
                return CancelJmapEmailSubmissionsResult::Err {
                    context,
                    err: err.into(),
                }
            }
        };

        let Some((name, args, _)) = response.method_responses.into_iter().next() else {
            return CancelJmapEmailSubmissionsResult::Err {
                context,
                err: CancelJmapEmailSubmissionsError::MissingResponse,
            };
        };

        if name == "error" {
            let err = serde_json::from_value::<JmapMethodError>(args)
                .unwrap_or(JmapMethodError::Unknown);
            return CancelJmapEmailSubmissionsResult::Err {
                context,
                err: err.into(),
            };
        }

        match serde_json::from_value::<EmailSubmissionCancelResponse>(args) {
            Ok(r) => CancelJmapEmailSubmissionsResult::Ok {
                context,
                new_state: r.new_state,
                updated: r.updated.unwrap_or_default(),
                not_updated: r.not_updated.unwrap_or_default(),
                keep_alive,
            },
            Err(err) => CancelJmapEmailSubmissionsResult::Err {
                context,
                err: CancelJmapEmailSubmissionsError::ParseResponse(err),
            },
        }
    }
}
