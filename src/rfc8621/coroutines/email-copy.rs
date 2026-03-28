//! I/O-free coroutine for the `Email/copy` method (RFC 8621 §4.10).

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
    rfc8621::types::email::{Email, EmailCopy},
};

/// Errors that can occur during the coroutine progression.
#[derive(Debug, Error)]
pub enum JmapEmailCopyError {
    #[error("Send JMAP request error: {0}")]
    Send(#[from] JmapSendError),
    #[error("Serialize Email/copy args error: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    #[error("Parse Email/copy response error: {0}")]
    ParseResponse(#[source] serde_json::Error),
    #[error("Missing Email/copy response in method_responses")]
    MissingResponse,
    #[error("JMAP Email/copy method error: {0}")]
    Method(#[from] JmapMethodError),
}

/// Result returned by the [`JmapEmailCopy`] coroutine.
#[derive(Debug)]
pub enum JmapEmailCopyResult {
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
        err: JmapEmailCopyError,
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
pub struct JmapEmailCopy {
    send: JmapSend,
}

impl JmapEmailCopy {
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        from_account_id: impl Into<String>,
        emails: HashMap<String, EmailCopy>,
    ) -> Result<Self, JmapEmailCopyError> {
        let account_id = session
            .primary_accounts
            .get(capabilities::MAIL)
            .cloned()
            .unwrap_or_default();
        let api_url = &session.api_url;

        let args = serde_json::to_value(EmailCopyArgs {
            from_account_id: from_account_id.into(),
            account_id,
            create: emails,
        })
        .map_err(JmapEmailCopyError::SerializeArgs)?;

        let mut batch = JmapBatch::new();
        batch.add("Email/copy", args);
        let request =
            batch.into_request(vec![capabilities::CORE.into(), capabilities::MAIL.into()]);

        Ok(Self {
            send: JmapSend::new(http_auth, api_url, request)?,
        })
    }

    pub fn resume(&mut self, arg: Option<StreamIo>) -> JmapEmailCopyResult {
        let (response, keep_alive) = match self.send.resume(arg) {
            JmapSendResult::Ok {
                response,
                keep_alive,
            } => (response, keep_alive),
            JmapSendResult::Io { io } => return JmapEmailCopyResult::Io { io },
            JmapSendResult::Err { err } => return JmapEmailCopyResult::Err { err: err.into() },
        };

        let Some((name, args, _)) = response.method_responses.into_iter().next() else {
            return JmapEmailCopyResult::Err {
                err: JmapEmailCopyError::MissingResponse,
            };
        };

        if name == "error" {
            let err =
                serde_json::from_value::<JmapMethodError>(args).unwrap_or(JmapMethodError::Unknown);
            return JmapEmailCopyResult::Err { err: err.into() };
        }

        match serde_json::from_value::<EmailCopyResponse>(args) {
            Ok(r) => JmapEmailCopyResult::Ok {
                new_state: r.new_state,
                created: r.created,
                not_created: r.not_created,
                keep_alive,
            },
            Err(err) => JmapEmailCopyResult::Err {
                err: JmapEmailCopyError::ParseResponse(err),
            },
        }
    }
}
