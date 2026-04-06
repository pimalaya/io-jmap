//! Generic I/O-free coroutine for the `Foo/query` method (RFC 8620 §5.5).

use alloc::{string::String, vec::Vec};
use io_socket::io::{SocketInput, SocketOutput};
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

use crate::rfc8620::{
    error::JmapMethodError,
    send::{JmapBatch, JmapSend, JmapSendError, JmapSendResult},
};

#[derive(Debug, Error)]
pub enum JmapQueryError {
    #[error("Send JMAP request error: {0}")]
    Send(#[from] JmapSendError),
    #[error("Serialize Foo/query args error: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    #[error("Parse Foo/query response error: {0}")]
    ParseResponse(#[source] serde_json::Error),
    #[error("Missing Foo/query response in method_responses")]
    MissingResponse,
    #[error("JMAP Foo/query method error: {0}")]
    Method(#[from] JmapMethodError),
}

#[derive(Debug)]
pub enum JmapQueryResult {
    Ok {
        query_state: String,
        can_calculate_changes: bool,
        position: u64,
        ids: Vec<String>,
        total: Option<u64>,
        limit: Option<u64>,
        keep_alive: bool,
    },
    Io {
        input: SocketInput,
    },
    Err {
        err: JmapQueryError,
    },
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct QueryArgs<'a, F: Serialize, S: Serialize> {
    account_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    filter: Option<F>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sort: Option<Vec<S>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    position: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    anchor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    anchor_offset: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    limit: Option<u64>,
    calculate_total: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct QueryResponse {
    query_state: String,
    #[serde(default)]
    can_calculate_changes: bool,
    #[serde(default)]
    position: u64,
    ids: Vec<String>,
    #[serde(default)]
    total: Option<u64>,
    #[serde(default)]
    limit: Option<u64>,
}

/// Generic I/O-free coroutine for the JMAP `Foo/query` method (RFC 8620 §5.5).
pub struct JmapQuery {
    send: JmapSend,
}

impl JmapQuery {
    pub fn new<F: Serialize, S: Serialize>(
        account_id: String,
        http_auth: &SecretString,
        api_url: &Url,
        method: impl Into<String>,
        capabilities: Vec<String>,
        filter: Option<F>,
        sort: Option<Vec<S>>,
        position: Option<u64>,
        anchor: Option<String>,
        anchor_offset: Option<i64>,
        limit: Option<u64>,
        calculate_total: bool,
    ) -> Result<Self, JmapQueryError> {
        let args = serde_json::to_value(QueryArgs {
            account_id: &account_id,
            filter,
            sort,
            position,
            anchor,
            anchor_offset,
            limit,
            calculate_total,
        })
        .map_err(JmapQueryError::SerializeArgs)?;

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

    pub fn resume(&mut self, arg: Option<SocketOutput>) -> JmapQueryResult {
        let (response, keep_alive) = match self.send.resume(arg) {
            JmapSendResult::Ok {
                response,
                keep_alive,
            } => (response, keep_alive),
            JmapSendResult::Io { input } => return JmapQueryResult::Io { input },
            JmapSendResult::Err { err } => return JmapQueryResult::Err { err: err.into() },
        };

        let Some((name, args, _)) = response.method_responses.into_iter().next() else {
            return JmapQueryResult::Err {
                err: JmapQueryError::MissingResponse,
            };
        };

        if name == "error" {
            let err =
                serde_json::from_value::<JmapMethodError>(args).unwrap_or(JmapMethodError::Unknown);
            return JmapQueryResult::Err { err: err.into() };
        }

        match serde_json::from_value::<QueryResponse>(args) {
            Ok(r) => JmapQueryResult::Ok {
                query_state: r.query_state,
                can_calculate_changes: r.can_calculate_changes,
                position: r.position,
                ids: r.ids,
                total: r.total,
                limit: r.limit,
                keep_alive,
            },
            Err(err) => JmapQueryResult::Err {
                err: JmapQueryError::ParseResponse(err),
            },
        }
    }
}
