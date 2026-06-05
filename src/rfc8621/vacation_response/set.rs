//! JMAP `VacationResponse/set` coroutine (RFC 8621 §8.3): updates the
//! singleton VacationResponse (id `"singleton"`).
//!
//! # Example
//!
//! ```rust,no_run
//! use io_jmap::{
//!     rfc8620::JmapSession,
//!     rfc8621::vacation_response::{VacationResponseUpdate, set::JmapVacationResponseSet},
//! };
//! use secrecy::SecretString;
//!
//! # fn demo(session: &JmapSession, patch: VacationResponseUpdate) {
//! let auth = SecretString::from("Bearer xyz");
//! let coroutine = JmapVacationResponseSet::new(session, &auth, patch).unwrap();
//! # let _ = coroutine;
//! # }
//! ```

use core::fmt;

use alloc::{collections::BTreeMap, string::String, vec};

use log::trace;
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    coroutine::*,
    jmap_try,
    rfc8620::{CORE_CAPABILITY, JmapBatch, JmapMethodError, JmapSession, send::*},
    rfc8621::{
        MAIL_CAPABILITY,
        vacation_response::{
            VACATION_RESPONSE_CAPABILITY, VacationResponse, VacationResponseUpdate,
        },
    },
};

/// Failure causes during a JMAP `VacationResponse/set` flow.
#[derive(Debug, Error)]
pub enum JmapVacationResponseSetError {
    #[error("JMAP VacationResponse/set failed: missing response in method_responses")]
    MissingResponse,
    #[error("JMAP VacationResponse/set failed: {0}")]
    Send(#[from] JmapSendError),
    #[error("JMAP VacationResponse/set failed: serialize args: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    #[error("JMAP VacationResponse/set failed: parse response: {0}")]
    ParseResponse(#[source] serde_json::Error),
    #[error("JMAP VacationResponse/set failed: {0}")]
    Method(#[from] JmapMethodError),
}

/// Successful terminal output of [`JmapVacationResponseSet`].
#[derive(Clone, Debug)]
pub struct JmapVacationResponseSetOutput {
    pub new_state: String,
    pub updated: Option<VacationResponse>,
    pub keep_alive: bool,
}

/// I/O-free coroutine for the JMAP `VacationResponse/set` method.
pub struct JmapVacationResponseSet {
    state: State,
}

impl JmapVacationResponseSet {
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        patch: VacationResponseUpdate,
    ) -> Result<Self, JmapVacationResponseSetError> {
        let account_id = session
            .primary_accounts
            .get(VACATION_RESPONSE_CAPABILITY)
            .or_else(|| session.primary_accounts.get(MAIL_CAPABILITY))
            .cloned()
            .unwrap_or_default();
        let api_url = &session.api_url;

        let args = serde_json::to_value(VacationResponseSetArgs {
            account_id,
            update: BTreeMap::from([("singleton", patch)]),
        })
        .map_err(JmapVacationResponseSetError::SerializeArgs)?;

        let mut using = vec![CORE_CAPABILITY.into(), MAIL_CAPABILITY.into()];
        if session
            .capabilities
            .contains_key(VACATION_RESPONSE_CAPABILITY)
        {
            using.push(VACATION_RESPONSE_CAPABILITY.into());
        }

        let mut batch = JmapBatch::new();
        batch.add("VacationResponse/set", args);
        let request = batch.into_request(using);

        Ok(Self {
            state: State::Send(JmapSend::new(http_auth, api_url, request)?),
        })
    }
}

impl JmapCoroutine for JmapVacationResponseSet {
    type Yield = JmapYield;
    type Return = Result<JmapVacationResponseSetOutput, JmapVacationResponseSetError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> JmapCoroutineState<Self::Yield, Self::Return> {
        trace!("VacationResponse/set: {}", self.state);
        match &mut self.state {
            State::Send(send) => {
                let JmapSendOutput {
                    response,
                    keep_alive,
                } = jmap_try!(send, arg);

                let Some((name, args, _)) = response.method_responses.into_iter().next() else {
                    return JmapCoroutineState::Complete(Err(
                        JmapVacationResponseSetError::MissingResponse,
                    ));
                };

                if name == "error" {
                    let err = serde_json::from_value::<JmapMethodError>(args)
                        .unwrap_or(JmapMethodError::Unknown);
                    return JmapCoroutineState::Complete(Err(err.into()));
                }

                match serde_json::from_value::<VacationResponseSetResponse>(args) {
                    Ok(r) => JmapCoroutineState::Complete(Ok(JmapVacationResponseSetOutput {
                        new_state: r.new_state,
                        updated: r.updated.unwrap_or_default().into_values().flatten().next(),
                        keep_alive,
                    })),
                    Err(err) => JmapCoroutineState::Complete(Err(
                        JmapVacationResponseSetError::ParseResponse(err),
                    )),
                }
            }
        }
    }
}

enum State {
    Send(JmapSend),
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Send(_) => f.write_str("send"),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VacationResponseSetArgs {
    account_id: String,
    update: BTreeMap<&'static str, VacationResponseUpdate>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct VacationResponseSetResponse {
    new_state: String,
    #[serde(default)]
    updated: Option<BTreeMap<String, Option<VacationResponse>>>,
}
