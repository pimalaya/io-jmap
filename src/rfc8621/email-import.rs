//! I/O-free coroutine for the `Email/import` method (RFC 8621 §4.9).

use alloc::{collections::BTreeMap, string::String, vec, vec::Vec};

use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    rfc8620::error::JmapMethodError,
    rfc8620::send::{JmapBatch, JmapSend, JmapSendError, JmapSendResult},
    rfc8620::session::JmapSession,
    rfc8621::capabilities,
    rfc8621::email::{Email, EmailImport, EmailImportError},
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
    /// The coroutine has successfully completed.
    Ok {
        new_state: String,
        created: BTreeMap<String, Email>,
        not_created: BTreeMap<String, EmailImportError>,
        keep_alive: bool,
    },
    /// The coroutine needs more bytes to be read from the socket.
    WantsRead,
    /// The coroutine wants the given bytes to be written to the socket.
    WantsWrite(Vec<u8>),
    /// The coroutine encountered an error.
    Err(JmapEmailImportError),
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EmailImportResponse {
    new_state: String,
    #[serde(default)]
    created: BTreeMap<String, Email>,
    #[serde(default)]
    not_created: BTreeMap<String, EmailImportError>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EmailImportArgs {
    account_id: String,
    emails: BTreeMap<String, EmailImport>,
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
        emails: BTreeMap<String, EmailImport>,
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

    /// Advances the coroutine.
    pub fn resume(&mut self, arg: Option<&[u8]>) -> JmapEmailImportResult {
        let (response, keep_alive) = match self.send.resume(arg) {
            JmapSendResult::Ok {
                response,
                keep_alive,
            } => (response, keep_alive),
            JmapSendResult::WantsRead => return JmapEmailImportResult::WantsRead,
            JmapSendResult::WantsWrite(bytes) => return JmapEmailImportResult::WantsWrite(bytes),
            JmapSendResult::Err(err) => return JmapEmailImportResult::Err(err.into()),
        };

        let Some((name, args, _)) = response.method_responses.into_iter().next() else {
            return JmapEmailImportResult::Err(JmapEmailImportError::MissingResponse);
        };

        if name == "error" {
            let err =
                serde_json::from_value::<JmapMethodError>(args).unwrap_or(JmapMethodError::Unknown);
            return JmapEmailImportResult::Err(err.into());
        }

        match serde_json::from_value::<EmailImportResponse>(args) {
            Ok(r) => JmapEmailImportResult::Ok {
                new_state: r.new_state,
                created: r.created,
                not_created: r.not_created,
                keep_alive,
            },
            Err(err) => JmapEmailImportResult::Err(JmapEmailImportError::ParseResponse(err)),
        }
    }
}
