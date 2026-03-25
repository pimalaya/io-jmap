//! I/O-free coroutine for the `Email/import` method (RFC 8621 §4.9).

use std::collections::HashMap;

use io_stream::io::StreamIo;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    context::JmapContext,
    coroutines::send::{JmapBatch, SendJmapRequest, SendJmapRequestError, SendJmapRequestResult},
    types::{email::{Email, EmailImport}, error::JmapMethodError, session::capabilities},
};

/// Errors that can occur during the coroutine progression.
#[derive(Debug, Error)]
pub enum ImportJmapEmailError {
    #[error("Send JMAP request error: {0}")]
    Send(#[from] SendJmapRequestError),
    #[error("Serialize Email/import args error: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    #[error("Parse Email/import response error: {0}")]
    ParseResponse(#[source] serde_json::Error),
    #[error("Missing Email/import response in method_responses")]
    MissingResponse,
    #[error("JMAP Email/import method error: {0}")]
    Method(#[from] JmapMethodError),
}

/// Per-email set error from import.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetError {
    #[serde(rename = "type")]
    pub error_type: String,
    pub description: Option<String>,
    #[serde(default)]
    pub properties: Vec<String>,
}

/// Result returned by the [`ImportJmapEmail`] coroutine.
#[derive(Debug)]
pub enum ImportJmapEmailResult {
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
        err: ImportJmapEmailError,
    },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EmailImportResponse {
    new_state: String,
    #[serde(default)]
    created: HashMap<String, Email>,
    #[serde(default)]
    not_created: HashMap<String, SetError>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EmailImportArgs {
    account_id: String,
    emails: HashMap<String, EmailImport>,
}

/// I/O-free coroutine for the JMAP `Email/import` method.
///
/// Imports raw RFC 5322 messages (previously uploaded as blobs) into
/// mailboxes. This is the JMAP equivalent of IMAP `APPEND`.
pub struct ImportJmapEmail {
    send: SendJmapRequest,
}

impl ImportJmapEmail {
    /// Creates a new coroutine.
    ///
    /// `emails` is a map from client-assigned ID to [`EmailImport`].
    pub fn new(
        context: JmapContext,
        emails: HashMap<String, EmailImport>,
    ) -> Result<Self, ImportJmapEmailError> {
        let account_id = context.account_id.clone().unwrap_or_default();
        let api_url = context
            .api_url()
            .cloned()
            .unwrap_or_else(|| "http://localhost".parse().unwrap());

        let args = serde_json::to_value(EmailImportArgs { account_id, emails })
            .map_err(ImportJmapEmailError::SerializeArgs)?;

        let mut batch = JmapBatch::new();
        batch.add("Email/import", args);
        let request = batch.into_request(vec![
            capabilities::CORE.into(),
            capabilities::MAIL.into(),
        ]);

        Ok(Self {
            send: SendJmapRequest::new(context, &api_url, request)?,
        })
    }

    /// Makes the coroutine progress.
    pub fn resume(&mut self, arg: Option<StreamIo>) -> ImportJmapEmailResult {
        let (context, response, keep_alive) = match self.send.resume(arg) {
            SendJmapRequestResult::Ok { context, response, keep_alive } => {
                (context, response, keep_alive)
            }
            SendJmapRequestResult::Io(io) => return ImportJmapEmailResult::Io(io),
            SendJmapRequestResult::Err { context, err } => {
                return ImportJmapEmailResult::Err {
                    context,
                    err: err.into(),
                }
            }
        };

        let Some((name, args, _)) = response.method_responses.into_iter().next() else {
            return ImportJmapEmailResult::Err {
                context,
                err: ImportJmapEmailError::MissingResponse,
            };
        };

        if name == "error" {
            let err = serde_json::from_value::<JmapMethodError>(args)
                .unwrap_or(JmapMethodError::Unknown);
            return ImportJmapEmailResult::Err {
                context,
                err: err.into(),
            };
        }

        match serde_json::from_value::<EmailImportResponse>(args) {
            Ok(r) => ImportJmapEmailResult::Ok {
                context,
                new_state: r.new_state,
                created: r.created,
                not_created: r.not_created,
                keep_alive,
            },
            Err(err) => ImportJmapEmailResult::Err {
                context,
                err: ImportJmapEmailError::ParseResponse(err),
            },
        }
    }
}
