//! Generic I/O-free coroutine for the `Foo/changes` method (RFC 8620 §5.2).

use alloc::{string::String, vec::Vec};

use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

use crate::coroutine::*;
use crate::rfc8620::{error::JmapMethodError, send::*};

#[derive(Debug, Error)]
pub enum JmapChangesError {
    #[error("Send JMAP request error: {0}")]
    Send(#[from] JmapSendError),
    #[error("Serialize Foo/changes args error: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    #[error("Parse Foo/changes response error: {0}")]
    ParseResponse(#[source] serde_json::Error),
    #[error("Missing Foo/changes response in method_responses")]
    MissingResponse,
    #[error("JMAP Foo/changes method error: {0}")]
    Method(#[from] JmapMethodError),
}

/// Successful output of [`JmapChanges`].
#[derive(Clone, Debug)]
pub struct JmapChangesOk {
    pub new_state: String,
    pub has_more_changes: bool,
    pub created: Vec<String>,
    pub updated: Vec<String>,
    pub destroyed: Vec<String>,
    pub keep_alive: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ChangesArgs<'a> {
    account_id: &'a str,
    since_state: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_changes: Option<u64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChangesResponse {
    new_state: String,
    has_more_changes: bool,
    created: Vec<String>,
    updated: Vec<String>,
    destroyed: Vec<String>,
}

/// Generic I/O-free coroutine for the JMAP `Foo/changes` method (RFC 8620 §5.2).
pub struct JmapChanges {
    send: JmapSend,
}

impl JmapChanges {
    pub fn new(
        account_id: String,
        http_auth: &SecretString,
        api_url: &Url,
        method: impl Into<String>,
        capabilities: Vec<String>,
        since_state: impl Into<String>,
        max_changes: Option<u64>,
    ) -> Result<Self, JmapChangesError> {
        let since_state = since_state.into();
        let args = serde_json::to_value(ChangesArgs {
            account_id: &account_id,
            since_state: &since_state,
            max_changes,
        })
        .map_err(JmapChangesError::SerializeArgs)?;

        let mut batch = JmapBatch::new();
        batch.add(method, args);
        let request = batch.into_request(capabilities);

        Ok(Self {
            send: JmapSend::new(http_auth, api_url, request)?,
        })
    }

    pub fn from_send(send: JmapSend) -> Self {
        Self { send }
    }
}

impl JmapCoroutine for JmapChanges {
    type Output = JmapChangesOk;
    type Error = JmapChangesError;

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
            return JmapCoroutineState::Err(JmapChangesError::MissingResponse);
        };

        if name == "error" {
            let err =
                serde_json::from_value::<JmapMethodError>(args).unwrap_or(JmapMethodError::Unknown);
            return JmapCoroutineState::Err(err.into());
        }

        match serde_json::from_value::<ChangesResponse>(args) {
            Ok(r) => JmapCoroutineState::Done(JmapChangesOk {
                new_state: r.new_state,
                has_more_changes: r.has_more_changes,
                created: r.created,
                updated: r.updated,
                destroyed: r.destroyed,
                keep_alive,
            }),
            Err(err) => JmapCoroutineState::Err(JmapChangesError::ParseResponse(err)),
        }
    }
}

/// Output of `Foo/changes` client methods
/// ([`JmapClientStd::mailbox_changes`],
/// [`JmapClientStd::email_changes`],
/// [`JmapClientStd::thread_changes`]).
///
/// [`JmapClientStd::mailbox_changes`]: crate::client::JmapClientStd::mailbox_changes
/// [`JmapClientStd::email_changes`]: crate::client::JmapClientStd::email_changes
/// [`JmapClientStd::thread_changes`]: crate::client::JmapClientStd::thread_changes
#[derive(Clone, Debug)]
pub struct JmapChangesOutput {
    pub new_state: String,
    pub has_more_changes: bool,
    pub created: Vec<String>,
    pub updated: Vec<String>,
    pub destroyed: Vec<String>,
}
