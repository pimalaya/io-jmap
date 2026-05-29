//! I/O-free coroutine for the `Email/import` method (RFC 8621 §4.9).

use alloc::{collections::BTreeMap, string::String, vec};

use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::coroutine::*;
use crate::{
    rfc8620::{error::JmapMethodError, send::*, session::JmapSession},
    rfc8621::{
        capabilities,
        email::{Email, EmailImport, EmailImportError},
    },
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

/// Successful terminal output of [`JmapEmailImport`].
#[derive(Clone, Debug)]
pub struct JmapEmailImportOutput {
    pub new_state: String,
    pub created: BTreeMap<String, Email>,
    pub not_created: BTreeMap<String, EmailImportError>,
    pub keep_alive: bool,
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
}

impl JmapCoroutine for JmapEmailImport {
    type Yield = JmapYield;
    type Return = Result<JmapEmailImportOutput, JmapEmailImportError>;

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
            return JmapCoroutineState::Complete(Err(JmapEmailImportError::MissingResponse));
        };

        if name == "error" {
            let err =
                serde_json::from_value::<JmapMethodError>(args).unwrap_or(JmapMethodError::Unknown);
            return JmapCoroutineState::Complete(Err(err.into()));
        }

        match serde_json::from_value::<EmailImportResponse>(args) {
            Ok(r) => JmapCoroutineState::Complete(Ok(JmapEmailImportOutput {
                new_state: r.new_state,
                created: r.created,
                not_created: r.not_created,
                keep_alive,
            })),
            Err(err) => JmapCoroutineState::Complete(Err(JmapEmailImportError::ParseResponse(err))),
        }
    }
}
