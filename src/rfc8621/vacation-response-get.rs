//! I/O-free coroutine for the `VacationResponse/get` method (RFC 8621 §8.2).

use alloc::{
    string::{String, ToString},
    vec,
    vec::Vec,
};

use secrecy::SecretString;
use serde::Serialize;
use thiserror::Error;

use crate::{
    rfc8620::get::{JmapGet, JmapGetError, JmapGetResult},
    rfc8620::send::{JmapBatch, JmapSend, JmapSendError},
    rfc8620::session::JmapSession,
    rfc8621::capabilities,
    rfc8621::vacation_response::VacationResponse,
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
    /// The coroutine has successfully completed.
    Ok {
        vacation_response: Option<VacationResponse>,
        new_state: String,
        keep_alive: bool,
    },
    /// The coroutine needs more bytes to be read from the socket.
    WantsRead,
    /// The coroutine wants the given bytes to be written to the socket.
    WantsWrite(Vec<u8>),
    /// The coroutine encountered an error.
    Err(JmapVacationResponseGetError),
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

    pub fn resume(&mut self, arg: Option<&[u8]>) -> JmapVacationResponseGetResult {
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
            JmapGetResult::WantsRead => JmapVacationResponseGetResult::WantsRead,
            JmapGetResult::WantsWrite(bytes) => JmapVacationResponseGetResult::WantsWrite(bytes),
            JmapGetResult::Err(err) => JmapVacationResponseGetResult::Err(err.into()),
        }
    }
}
