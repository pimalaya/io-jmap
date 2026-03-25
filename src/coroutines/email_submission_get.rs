//! I/O-free coroutine for the `EmailSubmission/get` method (RFC 8621 §7.2).

use io_stream::io::StreamIo;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    context::JmapContext,
    coroutines::send::{JmapBatch, SendJmapRequest, SendJmapRequestError, SendJmapRequestResult},
    types::{email_submission::EmailSubmission, error::JmapMethodError, session::capabilities},
};

/// Errors that can occur during the coroutine progression.
#[derive(Debug, Error)]
pub enum GetJmapEmailSubmissionsError {
    #[error("Send JMAP request error: {0}")]
    Send(#[from] SendJmapRequestError),
    #[error("Serialize EmailSubmission/get args error: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    #[error("Parse EmailSubmission/get response error: {0}")]
    ParseResponse(#[source] serde_json::Error),
    #[error("Missing EmailSubmission/get response in method_responses")]
    MissingResponse,
    #[error("JMAP EmailSubmission/get method error: {0}")]
    Method(#[from] JmapMethodError),
}

/// Result returned by the [`GetJmapEmailSubmissions`] coroutine.
#[derive(Debug)]
pub enum GetJmapEmailSubmissionsResult {
    Ok {
        context: JmapContext,
        submissions: Vec<EmailSubmission>,
        not_found: Vec<String>,
        new_state: String,
        keep_alive: bool,
    },
    Io(StreamIo),
    Err {
        context: JmapContext,
        err: GetJmapEmailSubmissionsError,
    },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EmailSubmissionGetResponse {
    list: Vec<EmailSubmission>,
    #[serde(default)]
    not_found: Vec<String>,
    state: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EmailSubmissionGetArgs {
    account_id: String,
    ids: Option<Vec<String>>,
}

/// I/O-free coroutine for the JMAP `EmailSubmission/get` method.
///
/// Fetches EmailSubmission objects by ID. Pass `ids: None` to fetch all.
pub struct GetJmapEmailSubmissions {
    send: SendJmapRequest,
}

impl GetJmapEmailSubmissions {
    pub fn new(
        context: JmapContext,
        ids: Option<Vec<String>>,
    ) -> Result<Self, GetJmapEmailSubmissionsError> {
        let account_id = context.account_id.clone().unwrap_or_default();
        let api_url = context
            .api_url()
            .cloned()
            .unwrap_or_else(|| "http://localhost".parse().unwrap());

        let args = serde_json::to_value(EmailSubmissionGetArgs { account_id, ids })
            .map_err(GetJmapEmailSubmissionsError::SerializeArgs)?;

        let mut batch = JmapBatch::new();
        batch.add("EmailSubmission/get", args);
        let request = batch.into_request(vec![
            capabilities::CORE.into(),
            capabilities::MAIL.into(),
            capabilities::SUBMISSION.into(),
        ]);

        Ok(Self {
            send: SendJmapRequest::new(context, &api_url, request)?,
        })
    }

    pub fn resume(&mut self, arg: Option<StreamIo>) -> GetJmapEmailSubmissionsResult {
        let (context, response, keep_alive) = match self.send.resume(arg) {
            SendJmapRequestResult::Ok { context, response, keep_alive } => {
                (context, response, keep_alive)
            }
            SendJmapRequestResult::Io(io) => return GetJmapEmailSubmissionsResult::Io(io),
            SendJmapRequestResult::Err { context, err } => {
                return GetJmapEmailSubmissionsResult::Err { context, err: err.into() }
            }
        };

        let Some((name, args, _)) = response.method_responses.into_iter().next() else {
            return GetJmapEmailSubmissionsResult::Err {
                context,
                err: GetJmapEmailSubmissionsError::MissingResponse,
            };
        };

        if name == "error" {
            let err = serde_json::from_value::<JmapMethodError>(args)
                .unwrap_or(JmapMethodError::Unknown);
            return GetJmapEmailSubmissionsResult::Err { context, err: err.into() };
        }

        match serde_json::from_value::<EmailSubmissionGetResponse>(args) {
            Ok(r) => GetJmapEmailSubmissionsResult::Ok {
                context,
                submissions: r.list,
                not_found: r.not_found,
                new_state: r.state,
                keep_alive,
            },
            Err(err) => GetJmapEmailSubmissionsResult::Err {
                context,
                err: GetJmapEmailSubmissionsError::ParseResponse(err),
            },
        }
    }
}
