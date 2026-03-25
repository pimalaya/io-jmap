//! I/O-free coroutine for the `Email/copy` method (RFC 8621 §4.10).

use std::collections::HashMap;

use io_stream::io::StreamIo;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    context::JmapContext,
    coroutines::send::{JmapBatch, SendJmapRequest, SendJmapRequestError, SendJmapRequestResult},
    types::{
        email::{Email, EmailCopy},
        error::JmapMethodError,
        session::capabilities,
    },
};

/// Errors that can occur during the coroutine progression.
#[derive(Debug, Error)]
pub enum CopyJmapEmailsError {
    #[error("Send JMAP request error: {0}")]
    Send(#[from] SendJmapRequestError),
    #[error("Serialize Email/copy args error: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    #[error("Parse Email/copy response error: {0}")]
    ParseResponse(#[source] serde_json::Error),
    #[error("Missing Email/copy response in method_responses")]
    MissingResponse,
    #[error("JMAP Email/copy method error: {0}")]
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

/// Result returned by the [`CopyJmapEmails`] coroutine.
#[derive(Debug)]
pub enum CopyJmapEmailsResult {
    Ok {
        context: JmapContext,
        new_state: String,
        created: HashMap<String, Email>,
        not_created: HashMap<String, SetError>,
        keep_alive: bool,
    },
    Io(StreamIo),
    Err {
        context: JmapContext,
        err: CopyJmapEmailsError,
    },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EmailCopyResponse {
    new_state: String,
    #[serde(default)]
    created: HashMap<String, Email>,
    #[serde(default)]
    not_created: HashMap<String, SetError>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EmailCopyArgs {
    from_account_id: String,
    account_id: String,
    create: HashMap<String, EmailCopy>,
}

/// I/O-free coroutine for the JMAP `Email/copy` method.
///
/// Copies emails from `from_account_id` into the current account.
/// `emails` maps client-assigned IDs to [`EmailCopy`] descriptors.
pub struct CopyJmapEmails {
    send: SendJmapRequest,
}

impl CopyJmapEmails {
    pub fn new(
        context: JmapContext,
        from_account_id: impl Into<String>,
        emails: HashMap<String, EmailCopy>,
    ) -> Result<Self, CopyJmapEmailsError> {
        let account_id = context.account_id.clone().unwrap_or_default();
        let api_url = context
            .api_url()
            .cloned()
            .unwrap_or_else(|| "http://localhost".parse().unwrap());

        let args = serde_json::to_value(EmailCopyArgs {
            from_account_id: from_account_id.into(),
            account_id,
            create: emails,
        })
        .map_err(CopyJmapEmailsError::SerializeArgs)?;

        let mut batch = JmapBatch::new();
        batch.add("Email/copy", args);
        let request =
            batch.into_request(vec![capabilities::CORE.into(), capabilities::MAIL.into()]);

        Ok(Self {
            send: SendJmapRequest::new(context, &api_url, request)?,
        })
    }

    pub fn resume(&mut self, arg: Option<StreamIo>) -> CopyJmapEmailsResult {
        let (context, response, keep_alive) = match self.send.resume(arg) {
            SendJmapRequestResult::Ok {
                context,
                response,
                keep_alive,
            } => (context, response, keep_alive),
            SendJmapRequestResult::Io(io) => return CopyJmapEmailsResult::Io(io),
            SendJmapRequestResult::Err { context, err } => {
                return CopyJmapEmailsResult::Err {
                    context,
                    err: err.into(),
                }
            }
        };

        let Some((name, args, _)) = response.method_responses.into_iter().next() else {
            return CopyJmapEmailsResult::Err {
                context,
                err: CopyJmapEmailsError::MissingResponse,
            };
        };

        if name == "error" {
            let err =
                serde_json::from_value::<JmapMethodError>(args).unwrap_or(JmapMethodError::Unknown);
            return CopyJmapEmailsResult::Err {
                context,
                err: err.into(),
            };
        }

        match serde_json::from_value::<EmailCopyResponse>(args) {
            Ok(r) => CopyJmapEmailsResult::Ok {
                context,
                new_state: r.new_state,
                created: r.created,
                not_created: r.not_created,
                keep_alive,
            },
            Err(err) => CopyJmapEmailsResult::Err {
                context,
                err: CopyJmapEmailsError::ParseResponse(err),
            },
        }
    }
}
