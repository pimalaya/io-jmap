//! I/O-free coroutine for the `VacationResponse/get` method (RFC 8621 §8.2).

use io_stream::io::StreamIo;
use secrecy::SecretString;
use serde::Serialize;
use thiserror::Error;

use crate::{
    rfc8620::coroutines::get::{JmapGet, JmapGetError, JmapGetResult},
    rfc8620::coroutines::send::{JmapBatch, JmapSend, JmapSendError},
    rfc8620::types::session::capabilities,
    rfc8620::types::session::JmapSession,
    rfc8621::types::vacation_response::VacationResponse,
};

/// Errors that can occur during the coroutine progression.
#[derive(Debug, Error)]
pub enum JmapVacationResponseGetError {
    #[error("Send JMAP request error: {0}")]
    Send(#[from] JmapSendError),
    #[error("Serialize VacationResponse/get args error: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    #[error("JMAP VacationResponse/get error: {0}")]
    Get(#[from] JmapGetError),
}

/// Result returned by the [`JmapVacationResponseGet`] coroutine.
#[derive(Debug)]
pub enum JmapVacationResponseGetResult {
    Ok {
        vacation_response: Option<VacationResponse>,
        new_state: String,
        keep_alive: bool,
    },
    Io {
        io: StreamIo,
    },
    Err {
        err: JmapVacationResponseGetError,
    },
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
pub struct JmapVacationResponseGet {
    get: JmapGet<VacationResponse>,
}

impl JmapVacationResponseGet {
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
    ) -> Result<Self, JmapVacationResponseGetError> {
        let account_id = session
            .primary_accounts
            .get(capabilities::VACATION_RESPONSE)
            .or_else(|| session.primary_accounts.get(capabilities::MAIL))
            .cloned()
            .unwrap_or_default();
        let api_url = &session.api_url;

        let args = serde_json::to_value(VacationResponseGetArgs {
            account_id,
            ids: vec!["singleton".to_string()],
        })
        .map_err(JmapVacationResponseGetError::SerializeArgs)?;

        let mut using = vec![capabilities::CORE.into(), capabilities::MAIL.into()];
        // Only declare the vacation-response capability if the server
        // advertises it.  Some servers (e.g. Fastmail) return HTTP 403 when
        // an unknown or unavailable capability appears in `using`.
        let has_vacation = session
            .capabilities
            .contains_key(capabilities::VACATION_RESPONSE);
        if has_vacation {
            using.push(capabilities::VACATION_RESPONSE.into());
        }

        let mut batch = JmapBatch::new();
        batch.add("VacationResponse/get", args);
        let request = batch.into_request(using);

        let send = JmapSend::new(http_auth, api_url, request)?;
        Ok(Self {
            get: JmapGet::from_send(send),
        })
    }

    pub fn resume(&mut self, arg: Option<StreamIo>) -> JmapVacationResponseGetResult {
        match self.get.resume(arg) {
            JmapGetResult::Ok {
                list,
                state,
                keep_alive,
                ..
            } => JmapVacationResponseGetResult::Ok {
                vacation_response: list.into_iter().next(),
                new_state: state,
                keep_alive,
            },
            JmapGetResult::Io { io } => JmapVacationResponseGetResult::Io { io },
            JmapGetResult::Err { err } => JmapVacationResponseGetResult::Err { err: err.into() },
        }
    }
}
