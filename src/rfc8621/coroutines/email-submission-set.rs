//! I/O-free coroutine for `EmailSubmission/set` (RFC 8621 §7.5).

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
    rfc8621::types::email_submission::{EmailSubmission, EmailSubmissionCreate},
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

/// Result returned by the [`JmapEmailSubmissionSet`] coroutine.
#[derive(Debug)]
pub enum JmapEmailSubmissionSetResult {
    Ok {
        new_state: String,
        created: HashMap<String, EmailSubmission>,
        not_created: HashMap<String, SetError>,
        keep_alive: bool,
    },
    Io {
        io: StreamIo,
    },
    Err {
        err: JmapEmailSubmissionSetError,
    },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EmailSubmissionSetResponse {
    new_state: String,
    #[serde(default)]
    created: Option<HashMap<String, EmailSubmission>>,
    #[serde(default)]
    not_created: Option<HashMap<String, SetError>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EmailSubmissionSetArgs {
    account_id: String,
    create: HashMap<String, EmailSubmissionCreate>,
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
        submissions: HashMap<String, EmailSubmissionCreate>,
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

    /// Makes the coroutine progress.
    pub fn resume(&mut self, arg: Option<StreamIo>) -> JmapEmailSubmissionSetResult {
        let (response, keep_alive) = match self.send.resume(arg) {
            JmapSendResult::Ok {
                response,
                keep_alive,
            } => (response, keep_alive),
            JmapSendResult::Io { io } => return JmapEmailSubmissionSetResult::Io { io },
            JmapSendResult::Err { err } => {
                return JmapEmailSubmissionSetResult::Err { err: err.into() }
            }
        };

        let Some((name, args, _)) = response.method_responses.into_iter().next() else {
            return JmapEmailSubmissionSetResult::Err {
                err: JmapEmailSubmissionSetError::MissingResponse,
            };
        };

        if name == "error" {
            let err =
                serde_json::from_value::<JmapMethodError>(args).unwrap_or(JmapMethodError::Unknown);
            return JmapEmailSubmissionSetResult::Err { err: err.into() };
        }

        match serde_json::from_value::<EmailSubmissionSetResponse>(args) {
            Ok(r) => JmapEmailSubmissionSetResult::Ok {
                new_state: r.new_state,
                created: r.created.unwrap_or_default(),
                not_created: r.not_created.unwrap_or_default(),
                keep_alive,
            },
            Err(err) => JmapEmailSubmissionSetResult::Err {
                err: JmapEmailSubmissionSetError::ParseResponse(err),
            },
        }
    }
}
