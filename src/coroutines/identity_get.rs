//! I/O-free coroutine for the `Identity/get` method (RFC 8621 §6.3).

use io_stream::io::StreamIo;
use serde::Deserialize;
use thiserror::Error;

use crate::{
    context::JmapContext,
    coroutines::send::{JmapBatch, SendJmapRequest, SendJmapRequestError, SendJmapRequestResult},
    types::{error::JmapMethodError, identity::Identity, session::capabilities},
};

/// Errors that can occur during the coroutine progression.
#[derive(Debug, Error)]
pub enum GetJmapIdentitiesError {
    #[error("Send JMAP request error: {0}")]
    Send(#[from] SendJmapRequestError),
    #[error("Parse Identity/get response error: {0}")]
    ParseResponse(#[source] serde_json::Error),
    #[error("Missing Identity/get response in method_responses")]
    MissingResponse,
    #[error("JMAP Identity/get method error: {0}")]
    Method(#[from] JmapMethodError),
}

/// Result returned by the [`GetJmapIdentities`] coroutine.
#[derive(Debug)]
pub enum GetJmapIdentitiesResult {
    Ok {
        context: JmapContext,
        identities: Vec<Identity>,
        not_found: Vec<String>,
        new_state: String,
        keep_alive: bool,
    },
    Io(StreamIo),
    Err {
        context: JmapContext,
        err: GetJmapIdentitiesError,
    },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IdentityGetResponse {
    list: Vec<Identity>,
    #[serde(default)]
    not_found: Vec<String>,
    state: String,
}

/// I/O-free coroutine for the JMAP `Identity/get` method.
///
/// Fetches sender identity objects. Pass `ids: None` to fetch all identities.
pub struct GetJmapIdentities {
    send: SendJmapRequest,
}

impl GetJmapIdentities {
    /// Creates a new coroutine.
    pub fn new(
        context: JmapContext,
        ids: Option<Vec<String>>,
    ) -> Result<Self, GetJmapIdentitiesError> {
        let account_id = context.account_id.clone().unwrap_or_default();
        let api_url = context
            .api_url()
            .cloned()
            .unwrap_or_else(|| "http://localhost".parse().unwrap());

        let mut args = serde_json::json!({ "accountId": account_id });

        if let Some(ids) = ids {
            args["ids"] = serde_json::json!(ids);
        } else {
            args["ids"] = serde_json::Value::Null;
        }

        let mut batch = JmapBatch::new();
        batch.add("Identity/get", args);
        let request = batch.into_request(vec![
            capabilities::CORE.into(),
            capabilities::MAIL.into(),
            capabilities::SUBMISSION.into(),
        ]);

        Ok(Self {
            send: SendJmapRequest::new(context, &api_url, request)?,
        })
    }

    /// Makes the coroutine progress.
    pub fn resume(&mut self, arg: Option<StreamIo>) -> GetJmapIdentitiesResult {
        let (context, response, keep_alive) = match self.send.resume(arg) {
            SendJmapRequestResult::Ok { context, response, keep_alive } => {
                (context, response, keep_alive)
            }
            SendJmapRequestResult::Io(io) => return GetJmapIdentitiesResult::Io(io),
            SendJmapRequestResult::Err { context, err } => {
                return GetJmapIdentitiesResult::Err {
                    context,
                    err: err.into(),
                }
            }
        };

        let Some((name, args, _)) = response.method_responses.into_iter().next() else {
            return GetJmapIdentitiesResult::Err {
                context,
                err: GetJmapIdentitiesError::MissingResponse,
            };
        };

        if name == "error" {
            let err = serde_json::from_value::<JmapMethodError>(args)
                .unwrap_or(JmapMethodError::Unknown);
            return GetJmapIdentitiesResult::Err {
                context,
                err: err.into(),
            };
        }

        match serde_json::from_value::<IdentityGetResponse>(args) {
            Ok(r) => GetJmapIdentitiesResult::Ok {
                context,
                identities: r.list,
                not_found: r.not_found,
                new_state: r.state,
                keep_alive,
            },
            Err(err) => GetJmapIdentitiesResult::Err {
                context,
                err: GetJmapIdentitiesError::ParseResponse(err),
            },
        }
    }
}
