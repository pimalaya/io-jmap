//! I/O-free coroutine for the `Email/import` method (RFC 8621 §4.9).

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
    rfc8621::types::email::{Email, EmailImport},
};

/// Errors that can occur during the coroutine progression.
#[derive(Debug, Error)]
pub enum JmapEmailImportError {
    #[error("Send JMAP request error: {0}")]
    Send(#[from] JmapSendError),
    #[error("Serialize Email/import args error: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    #[error("Parse Email/import response error: {0}")]
    ParseResponse(#[source] serde_json::Error),
    #[error("Missing Email/import response in method_responses")]
    MissingResponse,
    #[error("JMAP Email/import method error: {0}")]
    Method(#[from] JmapMethodError),
}

/// Result returned by the [`JmapEmailImport`] coroutine.
#[derive(Debug)]
pub enum JmapEmailImportResult {
    Ok {
        new_state: String,
        created: HashMap<String, Email>,
        not_created: HashMap<String, SetError>,
        keep_alive: bool,
    },
    Io {
        io: StreamIo,
    },
    Err {
        err: JmapEmailImportError,
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
pub struct JmapEmailImport {
    send: JmapSend,
}

impl JmapEmailImport {
    /// Creates a new coroutine.
    ///
    /// `emails` is a map from client-assigned ID to [`EmailImport`].
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        emails: HashMap<String, EmailImport>,
    ) -> Result<Self, JmapEmailImportError> {
        let account_id = session
            .primary_accounts
            .get(capabilities::MAIL)
            .cloned()
            .unwrap_or_default();
        let api_url = &session.api_url;

        let args = serde_json::to_value(EmailImportArgs { account_id, emails })
            .map_err(JmapEmailImportError::SerializeArgs)?;

        let mut batch = JmapBatch::new();
        batch.add("Email/import", args);
        let request =
            batch.into_request(vec![capabilities::CORE.into(), capabilities::MAIL.into()]);

        Ok(Self {
            send: JmapSend::new(http_auth, api_url, request)?,
        })
    }

    /// Makes the coroutine progress.
    pub fn resume(&mut self, arg: Option<StreamIo>) -> JmapEmailImportResult {
        let (response, keep_alive) = match self.send.resume(arg) {
            JmapSendResult::Ok {
                response,
                keep_alive,
            } => (response, keep_alive),
            JmapSendResult::Io { io } => return JmapEmailImportResult::Io { io },
            JmapSendResult::Err { err } => return JmapEmailImportResult::Err { err: err.into() },
        };

        let Some((name, args, _)) = response.method_responses.into_iter().next() else {
            return JmapEmailImportResult::Err {
                err: JmapEmailImportError::MissingResponse,
            };
        };

        if name == "error" {
            let err =
                serde_json::from_value::<JmapMethodError>(args).unwrap_or(JmapMethodError::Unknown);
            return JmapEmailImportResult::Err { err: err.into() };
        }

        match serde_json::from_value::<EmailImportResponse>(args) {
            Ok(r) => JmapEmailImportResult::Ok {
                new_state: r.new_state,
                created: r.created,
                not_created: r.not_created,
                keep_alive,
            },
            Err(err) => JmapEmailImportResult::Err {
                err: JmapEmailImportError::ParseResponse(err),
            },
        }
    }
}
