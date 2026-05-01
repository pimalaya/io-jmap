//! Generic I/O-free coroutine for the `Foo/changes` method (RFC 8620 §5.2).

use alloc::{string::String, vec::Vec};

use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

use crate::rfc8620::{
    error::JmapMethodError,
    send::{JmapBatch, JmapSend, JmapSendError, JmapSendResult},
};

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

#[derive(Debug)]
pub enum JmapChangesResult {
    /// The coroutine has successfully completed.
    Ok {
        new_state: String,
        has_more_changes: bool,
        created: Vec<String>,
        updated: Vec<String>,
        destroyed: Vec<String>,
        keep_alive: bool,
    },
    /// The coroutine needs more bytes to be read from the socket.
    WantsRead,
    /// The coroutine wants the given bytes to be written to the socket.
    WantsWrite(Vec<u8>),
    /// The coroutine encountered an error.
    Err(JmapChangesError),
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

    pub fn resume(&mut self, arg: Option<&[u8]>) -> JmapChangesResult {
        let (response, keep_alive) = match self.send.resume(arg) {
            JmapSendResult::Ok {
                response,
                keep_alive,
            } => (response, keep_alive),
            JmapSendResult::WantsRead => return JmapChangesResult::WantsRead,
            JmapSendResult::WantsWrite(bytes) => return JmapChangesResult::WantsWrite(bytes),
            JmapSendResult::Err(err) => return JmapChangesResult::Err(err.into()),
        };

        let Some((name, args, _)) = response.method_responses.into_iter().next() else {
            return JmapChangesResult::Err(JmapChangesError::MissingResponse);
        };

        if name == "error" {
            let err =
                serde_json::from_value::<JmapMethodError>(args).unwrap_or(JmapMethodError::Unknown);
            return JmapChangesResult::Err(err.into());
        }

        match serde_json::from_value::<ChangesResponse>(args) {
            Ok(r) => JmapChangesResult::Ok {
                new_state: r.new_state,
                has_more_changes: r.has_more_changes,
                created: r.created,
                updated: r.updated,
                destroyed: r.destroyed,
                keep_alive,
            },
            Err(err) => JmapChangesResult::Err(JmapChangesError::ParseResponse(err)),
        }
    }
}
