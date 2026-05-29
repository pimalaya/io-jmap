//! I/O-free coroutine for the `Email/copy` method (RFC 8621 §4.10).

use alloc::{collections::BTreeMap, string::String, vec};

use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::coroutine::*;
use crate::{
    rfc8620::{error::JmapMethodError, send::*, session::JmapSession},
    rfc8621::{
        capabilities,
        email::{Email, EmailCopy, EmailCopyError},
    },
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

/// Successful output of [`JmapEmailCopy`].
#[derive(Clone, Debug)]
pub struct JmapEmailCopyOk {
    pub new_state: String,
    pub created: BTreeMap<String, Email>,
    pub not_created: BTreeMap<String, EmailCopyError>,
    pub keep_alive: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EmailCopyResponse {
    new_state: String,
    #[serde(default)]
    created: BTreeMap<String, Email>,
    #[serde(default)]
    not_created: BTreeMap<String, EmailCopyError>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EmailCopyArgs {
    from_account_id: String,
    account_id: String,
    create: BTreeMap<String, EmailCopy>,
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
        emails: BTreeMap<String, EmailCopy>,
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
}

impl JmapCoroutine for JmapEmailCopy {
    type Output = JmapEmailCopyOk;
    type Error = JmapEmailCopyError;

    fn resume(&mut self, arg: Option<&[u8]>) -> JmapCoroutineState<Self::Output, Self::Error> {
        let (response, keep_alive) = match self.send.resume(arg) {
            JmapSendResult::Ok {
                response,
                keep_alive,
            } => (response, keep_alive),
            JmapSendResult::WantsRead => return JmapCoroutineState::WantsRead,
            JmapSendResult::WantsWrite(bytes) => return JmapCoroutineState::WantsWrite(bytes),
            JmapSendResult::Err(err) => return JmapCoroutineState::Err(err.into()),
        };

        let Some((name, args, _)) = response.method_responses.into_iter().next() else {
            return JmapCoroutineState::Err(JmapEmailCopyError::MissingResponse);
        };

        if name == "error" {
            let err =
                serde_json::from_value::<JmapMethodError>(args).unwrap_or(JmapMethodError::Unknown);
            return JmapCoroutineState::Err(err.into());
        }

        match serde_json::from_value::<EmailCopyResponse>(args) {
            Ok(r) => JmapCoroutineState::Done(JmapEmailCopyOk {
                new_state: r.new_state,
                created: r.created,
                not_created: r.not_created,
                keep_alive,
            }),
            Err(err) => JmapCoroutineState::Err(JmapEmailCopyError::ParseResponse(err)),
        }
    }
}

/// Output of the [`JmapClientStd::email_copy`] client method.
///
/// [`JmapClientStd::email_copy`]: crate::client::JmapClientStd::email_copy
#[derive(Clone, Debug)]
pub struct JmapEmailCopyOutput {
    pub new_state: String,
    pub created: BTreeMap<String, Email>,
    pub not_created: BTreeMap<String, EmailCopyError>,
}
