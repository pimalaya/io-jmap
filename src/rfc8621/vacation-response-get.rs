//! I/O-free coroutine for the `VacationResponse/get` method (RFC 8621 §8.2).

use alloc::{
    string::{String, ToString},
    vec,
    vec::Vec,
};

use secrecy::SecretString;
use serde::Serialize;
use thiserror::Error;

use crate::coroutine::*;
use crate::{
    rfc8620::{get::*, send::*, session::JmapSession},
    rfc8621::{capabilities, vacation_response::VacationResponse},
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

/// Successful terminal output of [`JmapVacationResponseGet`].
#[derive(Clone, Debug)]
pub struct JmapVacationResponseGetOutput {
    pub vacation_response: Option<VacationResponse>,
    pub new_state: String,
    pub keep_alive: bool,
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
}

impl JmapCoroutine for JmapVacationResponseGet {
    type Yield = JmapYield;
    type Return = Result<JmapVacationResponseGetOutput, JmapVacationResponseGetError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> JmapCoroutineState<Self::Yield, Self::Return> {
        match self.get.resume(arg) {
            JmapCoroutineState::Complete(Ok(JmapGetOutput {
                list,
                state,
                keep_alive,
                ..
            })) => JmapCoroutineState::Complete(Ok(JmapVacationResponseGetOutput {
                vacation_response: list.into_iter().next(),
                new_state: state,
                keep_alive,
            })),
            JmapCoroutineState::Complete(Err(err)) => JmapCoroutineState::Complete(Err(err.into())),
            JmapCoroutineState::Yielded(y) => JmapCoroutineState::Yielded(y),
        }
    }
}
