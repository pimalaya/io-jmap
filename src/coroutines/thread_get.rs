//! I/O-free coroutine for the `Thread/get` method (RFC 8621 §3.3).

use io_stream::io::StreamIo;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    context::JmapContext,
    coroutines::send::{JmapBatch, SendJmapRequest, SendJmapRequestError, SendJmapRequestResult},
    types::{error::JmapMethodError, session::capabilities, thread::Thread},
};

/// Errors that can occur during the coroutine progression.
#[derive(Debug, Error)]
pub enum GetJmapThreadsError {
    #[error("Send JMAP request error: {0}")]
    Send(#[from] SendJmapRequestError),
    #[error("Serialize Thread/get args error: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    #[error("Parse Thread/get response error: {0}")]
    ParseResponse(#[source] serde_json::Error),
    #[error("Missing Thread/get response in method_responses")]
    MissingResponse,
    #[error("JMAP Thread/get method error: {0}")]
    Method(#[from] JmapMethodError),
}

/// Result returned by the [`GetJmapThreads`] coroutine.
#[derive(Debug)]
pub enum GetJmapThreadsResult {
    Ok {
        context: JmapContext,
        threads: Vec<Thread>,
        not_found: Vec<String>,
        new_state: String,
        keep_alive: bool,
    },
    Io(StreamIo),
    Err {
        context: JmapContext,
        err: GetJmapThreadsError,
    },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ThreadGetResponse {
    list: Vec<Thread>,
    #[serde(default)]
    not_found: Vec<String>,
    state: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ThreadGetArgs {
    account_id: String,
    ids: Vec<String>,
}

/// I/O-free coroutine for the JMAP `Thread/get` method.
///
/// Fetches thread objects by ID, each containing an ordered list of
/// email IDs in the thread.
pub struct GetJmapThreads {
    send: SendJmapRequest,
}

impl GetJmapThreads {
    /// Creates a new coroutine.
    pub fn new(
        context: JmapContext,
        ids: Vec<String>,
    ) -> Result<Self, GetJmapThreadsError> {
        let account_id = context.account_id.clone().unwrap_or_default();
        let api_url = context
            .api_url()
            .cloned()
            .unwrap_or_else(|| "http://localhost".parse().unwrap());

        let args = serde_json::to_value(ThreadGetArgs { account_id, ids })
            .map_err(GetJmapThreadsError::SerializeArgs)?;

        let mut batch = JmapBatch::new();
        batch.add("Thread/get", args);
        let request = batch.into_request(vec![
            capabilities::CORE.into(),
            capabilities::MAIL.into(),
        ]);

        Ok(Self {
            send: SendJmapRequest::new(context, &api_url, request)?,
        })
    }

    /// Makes the coroutine progress.
    pub fn resume(&mut self, arg: Option<StreamIo>) -> GetJmapThreadsResult {
        let (context, response, keep_alive) = match self.send.resume(arg) {
            SendJmapRequestResult::Ok { context, response, keep_alive } => {
                (context, response, keep_alive)
            }
            SendJmapRequestResult::Io(io) => return GetJmapThreadsResult::Io(io),
            SendJmapRequestResult::Err { context, err } => {
                return GetJmapThreadsResult::Err {
                    context,
                    err: err.into(),
                }
            }
        };

        let Some((name, args, _)) = response.method_responses.into_iter().next() else {
            return GetJmapThreadsResult::Err {
                context,
                err: GetJmapThreadsError::MissingResponse,
            };
        };

        if name == "error" {
            let err = serde_json::from_value::<JmapMethodError>(args)
                .unwrap_or(JmapMethodError::Unknown);
            return GetJmapThreadsResult::Err {
                context,
                err: err.into(),
            };
        }

        match serde_json::from_value::<ThreadGetResponse>(args) {
            Ok(r) => GetJmapThreadsResult::Ok {
                context,
                threads: r.list,
                not_found: r.not_found,
                new_state: r.state,
                keep_alive,
            },
            Err(err) => GetJmapThreadsResult::Err {
                context,
                err: GetJmapThreadsError::ParseResponse(err),
            },
        }
    }
}
