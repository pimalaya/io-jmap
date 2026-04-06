//! I/O-free coroutine for the `VacationResponse/set` method (RFC 8621 §8.3).

use alloc::{collections::BTreeMap, string::String, vec};

use io_socket::io::{SocketInput, SocketOutput};
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    rfc8620::coroutines::send::{JmapBatch, JmapSend, JmapSendError, JmapSendResult},
    rfc8620::types::session::JmapSession,
    rfc8620::types::{error::JmapMethodError, session::capabilities},
    rfc8621::types::vacation_response::{VacationResponse, VacationResponseUpdate},
};

/// Errors that can occur during the coroutine progression.
#[derive(Debug, Error)]
pub enum JmapVacationResponseSetError {
    #[error("Send JMAP request error: {0}")]
    Send(#[from] JmapSendError),
    #[error("Serialize VacationResponse/set args error: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    #[error("Parse VacationResponse/set response error: {0}")]
    ParseResponse(#[source] serde_json::Error),
    #[error("Missing VacationResponse/set response in method_responses")]
    MissingResponse,
    #[error("JMAP VacationResponse/set method error: {0}")]
    Method(#[from] JmapMethodError),
}

/// Result returned by the [`JmapVacationResponseSet`] coroutine.
#[derive(Debug)]
pub enum JmapVacationResponseSetResult {
    Ok {
        new_state: String,
        updated: Option<VacationResponse>,
        keep_alive: bool,
    },
    Io {
        input: SocketInput,
    },
    Err {
        err: JmapVacationResponseSetError,
    },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct VacationResponseSetResponse {
    new_state: String,
    #[serde(default)]
    updated: Option<BTreeMap<String, Option<VacationResponse>>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VacationResponseSetArgs {
    account_id: String,
    update: BTreeMap<&'static str, VacationResponseUpdate>,
}

/// I/O-free coroutine for the JMAP `VacationResponse/set` method.
///
/// VacationResponse is a singleton: update it using id `"singleton"`.
pub struct JmapVacationResponseSet {
    send: JmapSend,
}

impl JmapVacationResponseSet {
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        patch: VacationResponseUpdate,
    ) -> Result<Self, JmapVacationResponseSetError> {
        let account_id = session
            .primary_accounts
            .get(capabilities::VACATION_RESPONSE)
            .or_else(|| session.primary_accounts.get(capabilities::MAIL))
            .cloned()
            .unwrap_or_default();
        let api_url = &session.api_url;

        let args = serde_json::to_value(VacationResponseSetArgs {
            account_id,
            update: BTreeMap::from([("singleton", patch)]),
        })
        .map_err(JmapVacationResponseSetError::SerializeArgs)?;

        let mut using = vec![capabilities::CORE.into(), capabilities::MAIL.into()];
        // Only declare the vacation-response capability if the server
        // advertises it.
        let has_vacation = session
            .capabilities
            .contains_key(capabilities::VACATION_RESPONSE);
        if has_vacation {
            using.push(capabilities::VACATION_RESPONSE.into());
        }

        let mut batch = JmapBatch::new();
        batch.add("VacationResponse/set", args);
        let request = batch.into_request(using);

        Ok(Self {
            send: JmapSend::new(http_auth, api_url, request)?,
        })
    }

    pub fn resume(&mut self, arg: Option<SocketOutput>) -> JmapVacationResponseSetResult {
        let (response, keep_alive) = match self.send.resume(arg) {
            JmapSendResult::Ok {
                response,
                keep_alive,
            } => (response, keep_alive),
            JmapSendResult::Io { input } => return JmapVacationResponseSetResult::Io { input },
            JmapSendResult::Err { err } => {
                return JmapVacationResponseSetResult::Err { err: err.into() };
            }
        };

        let Some((name, args, _)) = response.method_responses.into_iter().next() else {
            return JmapVacationResponseSetResult::Err {
                err: JmapVacationResponseSetError::MissingResponse,
            };
        };

        if name == "error" {
            let err =
                serde_json::from_value::<JmapMethodError>(args).unwrap_or(JmapMethodError::Unknown);
            return JmapVacationResponseSetResult::Err { err: err.into() };
        }

        match serde_json::from_value::<VacationResponseSetResponse>(args) {
            Ok(r) => JmapVacationResponseSetResult::Ok {
                new_state: r.new_state,
                updated: r.updated.unwrap_or_default().into_values().flatten().next(),
                keep_alive,
            },
            Err(err) => JmapVacationResponseSetResult::Err {
                err: JmapVacationResponseSetError::ParseResponse(err),
            },
        }
    }
}
