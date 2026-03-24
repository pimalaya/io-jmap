//! I/O-free coroutine for `EmailSubmission/set` (RFC 8621 §7.5).

use std::collections::HashMap;

use io_stream::io::StreamIo;
use serde::Deserialize;
use thiserror::Error;

use crate::{
    context::JmapContext,
    coroutines::send::{JmapBatch, SendJmapRequest, SendJmapRequestError, SendJmapRequestResult},
    types::{
        email_submission::{EmailSubmission, EmailSubmissionCreate},
        error::JmapMethodError,
        session::capabilities,
    },
};

/// Errors that can occur during the coroutine progression.
#[derive(Debug, Error)]
pub enum SubmitJmapEmailError {
    #[error("Send JMAP request error: {0}")]
    Send(#[from] SendJmapRequestError),
    #[error("Serialize EmailSubmission/set args error: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    #[error("Parse EmailSubmission/set response error: {0}")]
    ParseResponse(#[source] serde_json::Error),
    #[error("Missing EmailSubmission/set response in method_responses")]
    MissingResponse,
    #[error("JMAP EmailSubmission/set method error: {0}")]
    Method(#[from] JmapMethodError),
}

/// Per-object set error.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetError {
    #[serde(rename = "type")]
    pub error_type: String,
    pub description: Option<String>,
}

/// Result returned by the [`SubmitJmapEmail`] coroutine.
#[derive(Debug)]
pub enum SubmitJmapEmailResult {
    Ok {
        context: JmapContext,
        new_state: String,
        created: HashMap<String, EmailSubmission>,
        not_created: HashMap<String, SetError>,
        keep_alive: bool,
    },
    Io(StreamIo),
    Err {
        context: JmapContext,
        err: SubmitJmapEmailError,
    },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EmailSubmissionSetResponse {
    new_state: String,
    #[serde(default)]
    created: HashMap<String, EmailSubmission>,
    #[serde(default)]
    not_created: HashMap<String, SetError>,
}

/// I/O-free coroutine for the JMAP `EmailSubmission/set` method.
///
/// Submits emails for sending. This is the JMAP equivalent of SMTP
/// message submission.
pub struct SubmitJmapEmail {
    send: SendJmapRequest,
}

impl SubmitJmapEmail {
    /// Creates a new coroutine.
    ///
    /// `submissions` is a map from client-assigned ID to
    /// [`EmailSubmissionCreate`].
    pub fn new(
        context: JmapContext,
        submissions: HashMap<String, EmailSubmissionCreate>,
    ) -> Result<Self, SubmitJmapEmailError> {
        let account_id = context.account_id.clone().unwrap_or_default();
        let api_url = context
            .api_url()
            .cloned()
            .unwrap_or_else(|| "http://localhost".parse().unwrap());

        let create_json = serde_json::to_value(&submissions)
            .map_err(SubmitJmapEmailError::SerializeArgs)?;

        let args = serde_json::json!({
            "accountId": account_id,
            "create": create_json
        });

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
    pub fn resume(&mut self, arg: Option<StreamIo>) -> SubmitJmapEmailResult {
        let (context, response, keep_alive) = match self.send.resume(arg) {
            SendJmapRequestResult::Ok { context, response, keep_alive } => {
                (context, response, keep_alive)
            }
            SendJmapRequestResult::Io(io) => return SubmitJmapEmailResult::Io(io),
            SendJmapRequestResult::Err { context, err } => {
                return SubmitJmapEmailResult::Err {
                    context,
                    err: err.into(),
                }
            }
        };

        let Some((name, args, _)) = response.method_responses.into_iter().next() else {
            return SubmitJmapEmailResult::Err {
                context,
                err: SubmitJmapEmailError::MissingResponse,
            };
        };

        if name == "error" {
            let err = serde_json::from_value::<JmapMethodError>(args)
                .unwrap_or(JmapMethodError::Unknown);
            return SubmitJmapEmailResult::Err {
                context,
                err: err.into(),
            };
        }

        match serde_json::from_value::<EmailSubmissionSetResponse>(args) {
            Ok(r) => SubmitJmapEmailResult::Ok {
                context,
                new_state: r.new_state,
                created: r.created,
                not_created: r.not_created,
                keep_alive,
            },
            Err(err) => SubmitJmapEmailResult::Err {
                context,
                err: SubmitJmapEmailError::ParseResponse(err),
            },
        }
    }
}
