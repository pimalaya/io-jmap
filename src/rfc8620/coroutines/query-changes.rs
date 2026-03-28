//! Generic I/O-free coroutine for the `Foo/queryChanges` method (RFC 8620 §5.6).

use io_stream::io::StreamIo;
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::rfc8620::{
    coroutines::send::{JmapBatch, JmapSend, JmapSendError, JmapSendResult},
    types::{error::JmapMethodError, session::JmapSession},
};

/// Errors that can occur during the coroutine progression.
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

/// An item added to a query result, with its new position (RFC 8620 §5.6).
#[derive(Clone, Debug, Deserialize)]
pub struct AddedItem {
    /// The object ID.
    pub id: String,
    /// The zero-based index of this ID in the new query result.
    pub index: u64,
}

/// Result returned by the [`JmapQueryChanges`] coroutine.
#[derive(Debug)]
pub enum JmapQueryChangesResult {
    Ok {
        new_query_state: String,
        total: Option<u64>,
        removed: Vec<String>,
        added: Vec<AddedItem>,
        keep_alive: bool,
    },
    Io {
        io: StreamIo,
    },
    Err {
        err: JmapQueryChangesError,
    },
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
///
/// Returns the changes to a query result since `since_query_state`, expressed
/// as the IDs removed from and added to the result. The `added` items include
/// their new position in the result list.
///
/// Use `F` for the filter type and `S` for the sort comparator type — the same
/// filter and sort must be used as in the original `Foo/query` call.
pub struct JmapQueryChanges {
    send: JmapSend,
}

impl JmapQueryChanges {
    /// Creates a new coroutine.
    ///
    /// - `method`: JMAP method name, e.g. `"Email/queryChanges"`
    /// - `capabilities`: capability URNs to declare
    /// - `filter`: the same filter used in the original query
    /// - `sort`: the same sort used in the original query
    /// - `since_query_state`: the `queryState` from a previous `Foo/query` or `Foo/queryChanges`
    /// - `max_changes`: maximum number of changes to return; `None` for server default
    /// - `up_to_id`: stop reporting changes at this ID (exclusive upper bound)
    /// - `calculate_total`: whether to include the new total count in the response
    pub fn new<F: Serialize, S: Serialize>(
        session: &JmapSession,
        http_auth: &SecretString,
        method: impl Into<String>,
        capabilities: Vec<String>,
        filter: Option<F>,
        sort: Option<Vec<S>>,
        since_query_state: impl Into<String>,
        max_changes: Option<u64>,
        up_to_id: Option<String>,
        calculate_total: bool,
    ) -> Result<Self, JmapQueryChangesError> {
        let account_id = session.primary_account_id();
        let since_query_state = since_query_state.into();
        let api_url = &session.api_url;

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

    /// Makes the coroutine progress.
    pub fn resume(&mut self, arg: Option<StreamIo>) -> JmapQueryChangesResult {
        let (response, keep_alive) = match self.send.resume(arg) {
            JmapSendResult::Ok {
                response,
                keep_alive,
            } => (response, keep_alive),
            JmapSendResult::Io { io } => return JmapQueryChangesResult::Io { io },
            JmapSendResult::Err { err } => return JmapQueryChangesResult::Err { err: err.into() },
        };

        let Some((name, args, _)) = response.method_responses.into_iter().next() else {
            return JmapQueryChangesResult::Err {
                err: JmapQueryChangesError::MissingResponse,
            };
        };

        if name == "error" {
            let err =
                serde_json::from_value::<JmapMethodError>(args).unwrap_or(JmapMethodError::Unknown);
            return JmapQueryChangesResult::Err { err: err.into() };
        }

        match serde_json::from_value::<QueryChangesResponse>(args) {
            Ok(r) => JmapQueryChangesResult::Ok {
                new_query_state: r.new_query_state,
                total: r.total,
                removed: r.removed,
                added: r.added,
                keep_alive,
            },
            Err(err) => JmapQueryChangesResult::Err {
                err: JmapQueryChangesError::ParseResponse(err),
            },
        }
    }
}
