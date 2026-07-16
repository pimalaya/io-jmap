//! JMAP `VacationResponse/set` coroutine (RFC 8621 §8.3): updates the singleton
//! VacationResponse (id `"singleton"`).
//!
//! # Example
//!
//! ```rust,no_run
//! use std::{
//!     io::{Read, Write},
//!     net::TcpStream,
//! };
//!
//! use io_jmap::{
//!     coroutine::{JmapCoroutine, JmapCoroutineState, JmapYield},
//!     rfc8620::session::JmapSession,
//!     rfc8621::vacation_response::set::{JmapVacationResponseSet, JmapVacationResponseUpdate},
//! };
//! use secrecy::SecretString;
//!
//! // Ready stream needed (TCP-connected, TLS-negociated)
//! let mut stream = TcpStream::connect("api.example.com:443").unwrap();
//! let mut buf = [0u8; 4096];
//!
//! let session: JmapSession = serde_json::from_str(r#"{
//!     "username": "",
//!     "accounts": {},
//!     "primaryAccounts": {"urn:ietf:params:jmap:mail": "a1"},
//!     "capabilities": {},
//!     "apiUrl": "https://api.example.com/jmap/",
//!     "downloadUrl": "",
//!     "uploadUrl": "",
//!     "eventSourceUrl": "",
//!     "state": ""
//! }"#).unwrap();
//! let auth = SecretString::from("Bearer xyz");
//! let patch = JmapVacationResponseUpdate {
//!     is_enabled: Some(true),
//!     subject: Some("Out of office".into()),
//!     ..Default::default()
//! };
//! let mut coroutine = JmapVacationResponseSet::new(&session, &auth, patch).unwrap();
//! let mut arg = None;
//!
//! let out = loop {
//!     match coroutine.resume(arg.take()) {
//!         JmapCoroutineState::Yielded(JmapYield::WantsWrite(bytes)) => {
//!             stream.write_all(&bytes).unwrap();
//!         }
//!         JmapCoroutineState::Yielded(JmapYield::WantsRead) => {
//!             let n = stream.read(&mut buf).unwrap();
//!             arg = Some(&buf[..n]);
//!         }
//!         JmapCoroutineState::Complete(Ok(out)) => break out,
//!         JmapCoroutineState::Complete(Err(err)) => panic!("{err}"),
//!     }
//! };
//!
//! println!("new state {}", out.new_state);
//! ```

use alloc::{collections::BTreeMap, string::String, vec};

use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    coroutine::*,
    jmap_try,
    rfc8620::{
        JMAP_CORE_CAPABILITY, error::JmapMethodError, request::JmapBatch, send::*,
        session::JmapSession,
    },
    rfc8621::{
        JMAP_MAIL_CAPABILITY,
        vacation_response::{JMAP_VACATION_RESPONSE_CAPABILITY, JmapVacationResponse},
    },
};

/// Patch object for `VacationResponse/set` update (RFC 8621 §8).
///
/// Only `Some` fields are serialized; `None` fields are left unchanged.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapVacationResponseUpdate {
    /// Whether the vacation response is sent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_enabled: Option<bool>,
    /// RFC 3339 start of the vacation period.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_date: Option<String>,
    /// RFC 3339 end of the vacation period.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_date: Option<String>,
    /// Subject of the auto-reply message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    /// Plaintext body of the auto-reply message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_body: Option<String>,
    /// HTML body of the auto-reply message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub html_body: Option<String>,
}

/// Failure causes during a JMAP `VacationResponse/set` flow.
#[derive(Debug, Error)]
pub enum JmapVacationResponseSetError {
    /// The response carried no method response.
    #[error("JMAP VacationResponse/set failed: missing response in method_responses")]
    MissingResponse,
    /// The inner send coroutine failed.
    #[error("JMAP VacationResponse/set failed: {0}")]
    Send(#[from] JmapSendError),
    /// The method arguments could not be serialized.
    #[error("JMAP VacationResponse/set failed: serialize args: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    /// The method response could not be parsed.
    #[error("JMAP VacationResponse/set failed: parse response: {0}")]
    ParseResponse(#[source] serde_json::Error),
    /// The server returned a method-level error.
    #[error("JMAP VacationResponse/set failed: {0}")]
    Method(#[from] JmapMethodError),
}

/// Successful terminal output of [`JmapVacationResponseSet`].
#[derive(Clone, Debug)]
pub struct JmapVacationResponseSetOutput {
    /// The new server state after the call.
    pub new_state: String,
    /// The updated singleton, when the server echoed it back.
    pub updated: Option<JmapVacationResponse>,
    /// Whether the server indicated the connection can be reused.
    pub keep_alive: bool,
}

/// I/O-free coroutine for the JMAP `VacationResponse/set` method.
pub struct JmapVacationResponseSet {
    state: State,
}

impl JmapVacationResponseSet {
    /// Prepares the method call request and builds the coroutine.
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        patch: JmapVacationResponseUpdate,
    ) -> Result<Self, JmapVacationResponseSetError> {
        let account_id = session
            .primary_accounts
            .get(JMAP_VACATION_RESPONSE_CAPABILITY)
            .or_else(|| session.primary_accounts.get(JMAP_MAIL_CAPABILITY))
            .cloned()
            .unwrap_or_default();
        let api_url = &session.api_url;

        let args = serde_json::to_value(VacationResponseSetArgs {
            account_id,
            update: BTreeMap::from([("singleton", patch)]),
        })
        .map_err(JmapVacationResponseSetError::SerializeArgs)?;

        let mut using = vec![JMAP_CORE_CAPABILITY.into(), JMAP_MAIL_CAPABILITY.into()];
        if session
            .capabilities
            .contains_key(JMAP_VACATION_RESPONSE_CAPABILITY)
        {
            using.push(JMAP_VACATION_RESPONSE_CAPABILITY.into());
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VacationResponseSetArgs {
    account_id: String,
    update: BTreeMap<&'static str, JmapVacationResponseUpdate>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct VacationResponseSetResponse {
    new_state: String,
    #[serde(default)]
    updated: Option<BTreeMap<String, Option<JmapVacationResponse>>>,
}
