//! Generic I/O-free coroutine for the `Foo/queryChanges` method (RFC 8620 §5.6).

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
pub enum JmapQueryChangesError {
    #[error("Send JMAP request error: {0}")]
    Send(#[from] JmapSendError),
    #[error("Serialize Foo/queryChanges args error: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    #[error("Parse Foo/queryChanges response error: {0}")]
    ParseResponse(#[source] serde_json::Error),
    #[error("Missing Foo/queryChanges response in method_responses")]
    MissingResponse,
    #[error("JMAP Foo/queryChanges method error: {0}")]
    Method(#[from] JmapMethodError),
}

#[derive(Clone, Debug, Deserialize)]
pub struct AddedItem {
    pub id: String,
    pub index: u64,
}

#[derive(Debug)]
pub enum JmapQueryChangesResult {
    /// The coroutine has successfully completed.
    Ok {
        new_query_state: String,
        total: Option<u64>,
        removed: Vec<String>,
        added: Vec<AddedItem>,
        keep_alive: bool,
    },
    /// The coroutine needs more bytes to be read from the socket.
    WantsRead,
    /// The coroutine wants the given bytes to be written to the socket.
    WantsWrite(Vec<u8>),
    /// The coroutine encountered an error.
    Err(JmapQueryChangesError),
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct QueryChangesArgs<'a, F: Serialize, S: Serialize> {
    account_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    filter: Option<F>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sort: Option<Vec<S>>,
    since_query_state: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_changes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    up_to_id: Option<String>,
    calculate_total: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct QueryChangesResponse {
    new_query_state: String,
    #[serde(default)]
    total: Option<u64>,
    removed: Vec<String>,
    added: Vec<AddedItem>,
}

/// Generic I/O-free coroutine for the JMAP `Foo/queryChanges` method (RFC 8620 §5.6).
pub struct JmapQueryChanges {
    send: JmapSend,
}

impl JmapQueryChanges {
    pub fn new<F: Serialize, S: Serialize>(
        account_id: String,
        http_auth: &SecretString,
        api_url: &Url,
        method: impl Into<String>,
        capabilities: Vec<String>,
        filter: Option<F>,
        sort: Option<Vec<S>>,
        since_query_state: impl Into<String>,
        max_changes: Option<u64>,
        up_to_id: Option<String>,
        calculate_total: bool,
    ) -> Result<Self, JmapQueryChangesError> {
        let since_query_state = since_query_state.into();
        let args = serde_json::to_value(QueryChangesArgs {
            account_id: &account_id,
            filter,
            sort,
            since_query_state: &since_query_state,
            max_changes,
            up_to_id,
            calculate_total,
        })
        .map_err(JmapQueryChangesError::SerializeArgs)?;

        let mut batch = JmapBatch::new();
        batch.add(method, args);
        let request = batch.into_request(capabilities);

        Ok(Self {
            send: JmapSend::new(http_auth, api_url, request)?,
        })
    }

    pub fn resume(&mut self, arg: Option<&[u8]>) -> JmapQueryChangesResult {
        let (response, keep_alive) = match self.send.resume(arg) {
            JmapSendResult::Ok {
                response,
                keep_alive,
            } => (response, keep_alive),
            JmapSendResult::WantsRead => return JmapQueryChangesResult::WantsRead,
            JmapSendResult::WantsWrite(bytes) => return JmapQueryChangesResult::WantsWrite(bytes),
            JmapSendResult::Err(err) => return JmapQueryChangesResult::Err(err.into()),
        };

        let Some((name, args, _)) = response.method_responses.into_iter().next() else {
            return JmapQueryChangesResult::Err(JmapQueryChangesError::MissingResponse);
        };

        if name == "error" {
            let err =
                serde_json::from_value::<JmapMethodError>(args).unwrap_or(JmapMethodError::Unknown);
            return JmapQueryChangesResult::Err(err.into());
        }

        match serde_json::from_value::<QueryChangesResponse>(args) {
            Ok(r) => JmapQueryChangesResult::Ok {
                new_query_state: r.new_query_state,
                total: r.total,
                removed: r.removed,
                added: r.added,
                keep_alive,
            },
            Err(err) => JmapQueryChangesResult::Err(JmapQueryChangesError::ParseResponse(err)),
        }
    }
}
