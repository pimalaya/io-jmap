//! I/O-free coroutine for the `VacationResponse/set` method (RFC 8621 §8.3).

use io_stream::io::StreamIo;
use serde::Deserialize;
use thiserror::Error;

use crate::{
    context::JmapContext,
    coroutines::send::{JmapBatch, SendJmapRequest, SendJmapRequestError, SendJmapRequestResult},
    types::{
        error::JmapMethodError,
        session::capabilities,
        vacation_response::{VacationResponse, VacationResponseUpdate},
    },
};

/// Errors that can occur during the coroutine progression.
#[derive(Debug, Error)]
pub enum SetJmapVacationResponseError {
    #[error("Send JMAP request error: {0}")]
    Send(#[from] SendJmapRequestError),
    #[error("Serialize VacationResponse/set args error: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    #[error("Parse VacationResponse/set response error: {0}")]
    ParseResponse(#[source] serde_json::Error),
    #[error("Missing VacationResponse/set response in method_responses")]
    MissingResponse,
    #[error("JMAP VacationResponse/set method error: {0}")]
    Method(#[from] JmapMethodError),
}

/// Per-object set error.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetError {
    #[serde(rename = "type")]
    pub error_type: String,
    pub description: Option<String>,
}

/// Result returned by the [`SetJmapVacationResponse`] coroutine.
#[derive(Debug)]
pub enum SetJmapVacationResponseResult {
    Ok {
        context: JmapContext,
        new_state: String,
        updated: Option<VacationResponse>,
        keep_alive: bool,
    },
    Io(StreamIo),
    Err {
        context: JmapContext,
        err: SetJmapVacationResponseError,
    },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct VacationResponseSetResponse {
    new_state: String,
    #[serde(default)]
    updated: std::collections::HashMap<String, Option<VacationResponse>>,
}

/// I/O-free coroutine for the JMAP `VacationResponse/set` method.
///
/// VacationResponse is a singleton: update it using id `"singleton"`.
pub struct SetJmapVacationResponse {
    send: SendJmapRequest,
}

impl SetJmapVacationResponse {
    pub fn new(
        context: JmapContext,
        patch: VacationResponseUpdate,
    ) -> Result<Self, SetJmapVacationResponseError> {
        let account_id = context.account_id.clone().unwrap_or_default();
        let api_url = context
            .api_url()
            .cloned()
            .unwrap_or_else(|| "http://localhost".parse().unwrap());

        let patch_json = serde_json::to_value(&patch)
            .map_err(SetJmapVacationResponseError::SerializeArgs)?;

        let args = serde_json::json!({
            "accountId": account_id,
            "update": { "singleton": patch_json }
        });

        let mut batch = JmapBatch::new();
        batch.add("VacationResponse/set", args);
        let request = batch.into_request(vec![
            capabilities::CORE.into(),
            capabilities::MAIL.into(),
            capabilities::VACATION_RESPONSE.into(),
        ]);

        Ok(Self {
            send: SendJmapRequest::new(context, &api_url, request)?,
        })
    }

    pub fn resume(&mut self, arg: Option<StreamIo>) -> SetJmapVacationResponseResult {
        let (context, response, keep_alive) = match self.send.resume(arg) {
            SendJmapRequestResult::Ok { context, response, keep_alive } => {
                (context, response, keep_alive)
            }
            SendJmapRequestResult::Io(io) => return SetJmapVacationResponseResult::Io(io),
            SendJmapRequestResult::Err { context, err } => {
                return SetJmapVacationResponseResult::Err { context, err: err.into() }
            }
        };

        let Some((name, args, _)) = response.method_responses.into_iter().next() else {
            return SetJmapVacationResponseResult::Err {
                context,
                err: SetJmapVacationResponseError::MissingResponse,
            };
        };

        if name == "error" {
            let err = serde_json::from_value::<JmapMethodError>(args)
                .unwrap_or(JmapMethodError::Unknown);
            return SetJmapVacationResponseResult::Err { context, err: err.into() };
        }

        match serde_json::from_value::<VacationResponseSetResponse>(args) {
            Ok(r) => SetJmapVacationResponseResult::Ok {
                context,
                new_state: r.new_state,
                updated: r.updated.into_values().flatten().next(),
                keep_alive,
            },
            Err(err) => SetJmapVacationResponseResult::Err {
                context,
                err: SetJmapVacationResponseError::ParseResponse(err),
            },
        }
    }
}
