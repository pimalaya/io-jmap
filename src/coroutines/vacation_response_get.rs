//! I/O-free coroutine for the `VacationResponse/get` method (RFC 8621 §8.2).

use io_stream::io::StreamIo;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    context::JmapContext,
    coroutines::send::{JmapBatch, SendJmapRequest, SendJmapRequestError, SendJmapRequestResult},
    types::{error::JmapMethodError, session::capabilities, vacation_response::VacationResponse},
};

/// Errors that can occur during the coroutine progression.
#[derive(Debug, Error)]
pub enum GetJmapVacationResponseError {
    #[error("Send JMAP request error: {0}")]
    Send(#[from] SendJmapRequestError),
    #[error("Serialize VacationResponse/get args error: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    #[error("Parse VacationResponse/get response error: {0}")]
    ParseResponse(#[source] serde_json::Error),
    #[error("Missing VacationResponse/get response in method_responses")]
    MissingResponse,
    #[error("JMAP VacationResponse/get method error: {0}")]
    Method(#[from] JmapMethodError),
}

/// Result returned by the [`GetJmapVacationResponse`] coroutine.
#[derive(Debug)]
pub enum GetJmapVacationResponseResult {
    Ok {
        context: JmapContext,
        vacation_response: Option<VacationResponse>,
        new_state: String,
        keep_alive: bool,
    },
    Io(StreamIo),
    Err {
        context: JmapContext,
        err: GetJmapVacationResponseError,
    },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct VacationResponseGetResponse {
    list: Vec<VacationResponse>,
    state: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VacationResponseGetArgs {
    account_id: String,
    ids: Vec<String>,
}

/// I/O-free coroutine for the JMAP `VacationResponse/get` method.
///
/// The `ids` argument should be `["singleton"]` or `null` to fetch the
/// single VacationResponse object for the account.
pub struct GetJmapVacationResponse {
    send: SendJmapRequest,
}

impl GetJmapVacationResponse {
    pub fn new(context: JmapContext) -> Result<Self, GetJmapVacationResponseError> {
        let account_id = context
            .account_id_for(capabilities::VACATION_RESPONSE)
            .unwrap_or_default();
        let api_url = context
            .api_url()
            .cloned()
            .unwrap_or_else(|| "http://localhost".parse().unwrap());

        let args = serde_json::to_value(VacationResponseGetArgs {
            account_id,
            ids: vec!["singleton".to_string()],
        })
        .map_err(GetJmapVacationResponseError::SerializeArgs)?;

        let mut using = vec![capabilities::CORE.into(), capabilities::MAIL.into()];
        // Only declare the vacation-response capability if the server
        // advertises it.  Some servers (e.g. Fastmail) return HTTP 403 when
        // an unknown or unavailable capability appears in `using`.
        let has_vacation = context
            .session
            .as_ref()
            .map(|s| s.capabilities.contains_key(capabilities::VACATION_RESPONSE))
            .unwrap_or(true);
        if has_vacation {
            using.push(capabilities::VACATION_RESPONSE.into());
        }

        let mut batch = JmapBatch::new();
        batch.add("VacationResponse/get", args);
        let request = batch.into_request(using);

        Ok(Self {
            send: SendJmapRequest::new(context, &api_url, request)?,
        })
    }

    pub fn resume(&mut self, arg: Option<StreamIo>) -> GetJmapVacationResponseResult {
        let (context, response, keep_alive) = match self.send.resume(arg) {
            SendJmapRequestResult::Ok { context, response, keep_alive } => {
                (context, response, keep_alive)
            }
            SendJmapRequestResult::Io(io) => return GetJmapVacationResponseResult::Io(io),
            SendJmapRequestResult::Err { context, err } => {
                return GetJmapVacationResponseResult::Err { context, err: err.into() }
            }
        };

        let Some((name, args, _)) = response.method_responses.into_iter().next() else {
            return GetJmapVacationResponseResult::Err {
                context,
                err: GetJmapVacationResponseError::MissingResponse,
            };
        };

        if name == "error" {
            let err = serde_json::from_value::<JmapMethodError>(args)
                .unwrap_or(JmapMethodError::Unknown);
            return GetJmapVacationResponseResult::Err { context, err: err.into() };
        }

        match serde_json::from_value::<VacationResponseGetResponse>(args) {
            Ok(r) => GetJmapVacationResponseResult::Ok {
                context,
                vacation_response: r.list.into_iter().next(),
                new_state: r.state,
                keep_alive,
            },
            Err(err) => GetJmapVacationResponseResult::Err {
                context,
                err: GetJmapVacationResponseError::ParseResponse(err),
            },
        }
    }
}
