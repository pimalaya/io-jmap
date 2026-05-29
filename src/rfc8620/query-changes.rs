//! Generic I/O-free coroutine for the `Foo/queryChanges` method (RFC 8620 §5.6).

use alloc::{string::String, vec::Vec};

use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

use crate::coroutine::*;
use crate::rfc8620::{error::JmapMethodError, send::*};

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

/// Successful terminal output of [`JmapQueryChanges`].
#[derive(Clone, Debug)]
pub struct JmapQueryChangesOutput {
    pub new_query_state: String,
    pub total: Option<u64>,
    pub removed: Vec<String>,
    pub added: Vec<AddedItem>,
    pub keep_alive: bool,
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
}

impl JmapCoroutine for JmapQueryChanges {
    type Yield = JmapYield;
    type Return = Result<JmapQueryChangesOutput, JmapQueryChangesError>;

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
            return JmapCoroutineState::Complete(Err(JmapQueryChangesError::MissingResponse));
        };

        if name == "error" {
            let err =
                serde_json::from_value::<JmapMethodError>(args).unwrap_or(JmapMethodError::Unknown);
            return JmapCoroutineState::Complete(Err(err.into()));
        }

        match serde_json::from_value::<QueryChangesResponse>(args) {
            Ok(r) => JmapCoroutineState::Complete(Ok(JmapQueryChangesOutput {
                new_query_state: r.new_query_state,
                total: r.total,
                removed: r.removed,
                added: r.added,
                keep_alive,
            })),
            Err(err) => {
                JmapCoroutineState::Complete(Err(JmapQueryChangesError::ParseResponse(err)))
            }
        }
    }
}
