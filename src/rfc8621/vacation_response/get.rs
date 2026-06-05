//! JMAP `VacationResponse/get` coroutine (RFC 8621 §8.2): wraps the
//! generic [`JmapGet`] for the singleton `VacationResponse` object.
//!
//! # Example
//!
//! ```rust,no_run
//! use io_jmap::{
//!     rfc8620::JmapSession,
//!     rfc8621::vacation_response::get::JmapVacationResponseGet,
//! };
//! use secrecy::SecretString;
//!
//! # fn demo(session: &JmapSession) {
//! let auth = SecretString::from("Bearer xyz");
//! let coroutine = JmapVacationResponseGet::new(session, &auth).unwrap();
//! # let _ = coroutine;
//! # }
//! ```

use core::fmt;

use alloc::{
    string::{String, ToString},
    vec,
    vec::Vec,
};

use log::trace;
use secrecy::SecretString;
use serde::Serialize;
use thiserror::Error;

use crate::coroutine::*;
use crate::jmap_try;
use crate::{
    rfc8620::{CORE_CAPABILITY, JmapBatch, JmapSession, get::*, send::*},
    rfc8621::{
        MAIL_CAPABILITY,
        vacation_response::{VACATION_RESPONSE_CAPABILITY, VacationResponse},
    },
};

/// Failure causes during a JMAP `VacationResponse/get` flow.
#[derive(Debug, Error)]
pub enum JmapVacationResponseGetError {
    #[error("JMAP VacationResponse/get failed: {0}")]
    Send(#[from] JmapSendError),
    #[error("JMAP VacationResponse/get failed: serialize args: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    #[error("JMAP VacationResponse/get failed: {0}")]
    Get(#[from] JmapGetError),
}

/// Successful terminal output of [`JmapVacationResponseGet`].
#[derive(Clone, Debug)]
pub struct JmapVacationResponseGetOutput {
    pub vacation_response: Option<VacationResponse>,
    pub new_state: String,
    pub keep_alive: bool,
}

/// I/O-free coroutine for the JMAP `VacationResponse/get` method.
pub struct JmapVacationResponseGet {
    state: State,
}

impl JmapVacationResponseGet {
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
    ) -> Result<Self, JmapVacationResponseGetError> {
        let account_id = session
            .primary_accounts
            .get(VACATION_RESPONSE_CAPABILITY)
            .or_else(|| session.primary_accounts.get(MAIL_CAPABILITY))
            .cloned()
            .unwrap_or_default();
        let api_url = &session.api_url;

        let args = serde_json::to_value(VacationResponseGetArgs {
            account_id,
            ids: vec!["singleton".to_string()],
        })
        .map_err(JmapVacationResponseGetError::SerializeArgs)?;

        let mut using = vec![CORE_CAPABILITY.into(), MAIL_CAPABILITY.into()];
        if session
            .capabilities
            .contains_key(VACATION_RESPONSE_CAPABILITY)
        {
            using.push(VACATION_RESPONSE_CAPABILITY.into());
        }

        let mut batch = JmapBatch::new();
        batch.add("VacationResponse/get", args);
        let request = batch.into_request(using);

        let send = JmapSend::new(http_auth, api_url, request)?;
        Ok(Self {
            state: State::Get(JmapGet::from_send(send)),
        })
    }
}

impl JmapCoroutine for JmapVacationResponseGet {
    type Yield = JmapYield;
    type Return = Result<JmapVacationResponseGetOutput, JmapVacationResponseGetError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> JmapCoroutineState<Self::Yield, Self::Return> {
        trace!("VacationResponse/get: {}", self.state);
        match &mut self.state {
            State::Get(get) => {
                let JmapGetOutput {
                    list,
                    state,
                    keep_alive,
                    ..
                } = jmap_try!(get, arg);
                JmapCoroutineState::Complete(Ok(JmapVacationResponseGetOutput {
                    vacation_response: list.into_iter().next(),
                    new_state: state,
                    keep_alive,
                }))
            }
        }
    }
}

enum State {
    Get(JmapGet<VacationResponse>),
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Get(_) => f.write_str("get"),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VacationResponseGetArgs {
    account_id: String,
    ids: Vec<String>,
}
