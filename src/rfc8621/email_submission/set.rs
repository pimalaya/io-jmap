//! JMAP `EmailSubmission/set` coroutine (RFC 8621 §7.5): submits emails for
//! sending. JMAP equivalent of SMTP message submission.
//!
//! # Example
//!
//! ```rust,no_run
//! use std::{
//!     collections::BTreeMap,
//!     io::{Read, Write},
//!     net::TcpStream,
//! };
//!
//! use io_jmap::{
//!     coroutine::{JmapCoroutine, JmapCoroutineState, JmapYield},
//!     rfc8620::JmapSession,
//!     rfc8621::email_submission::{JmapEmailSubmissionCreate, set::JmapEmailSubmissionSet},
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
//! let mut submissions = BTreeMap::new();
//! submissions.insert(
//!     "c1".to_string(),
//!     JmapEmailSubmissionCreate {
//!         identity_id: "id1".into(),
//!         email_id: "e1".into(),
//!         envelope: None,
//!     },
//! );
//! let mut coroutine = JmapEmailSubmissionSet::new(&session, &auth, submissions).unwrap();
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
//! println!("{} created", out.created.len());
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
        email_submission::{
            JmapEmailSubmission, JmapEmailSubmissionCreate, JmapEmailSubmissionSetItemError,
            SUBMISSION_CAPABILITY,
        },
    },
};

/// Failure causes during a JMAP `EmailSubmission/set` flow.
#[derive(Debug, Error)]
pub enum JmapEmailSubmissionSetError {
    #[error("JMAP EmailSubmission/set failed: missing response in method_responses")]
    MissingResponse,
    #[error("JMAP EmailSubmission/set failed: {0}")]
    Send(#[from] JmapSendError),
    #[error("JMAP EmailSubmission/set failed: serialize args: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    #[error("JMAP EmailSubmission/set failed: parse response: {0}")]
    ParseResponse(#[source] serde_json::Error),
    #[error("JMAP EmailSubmission/set failed: {0}")]
    Method(#[from] JmapMethodError),
}

/// Successful terminal output of [`JmapEmailSubmissionSet`].
#[derive(Clone, Debug)]
pub struct JmapEmailSubmissionSetOutput {
    pub new_state: String,
    pub created: BTreeMap<String, JmapEmailSubmission>,
    pub not_created: BTreeMap<String, JmapEmailSubmissionSetItemError>,
    pub keep_alive: bool,
}

/// I/O-free coroutine for the JMAP `EmailSubmission/set` method.
pub struct JmapEmailSubmissionSet {
    state: State,
}

impl JmapEmailSubmissionSet {
    /// `submissions` maps client-assigned IDs to [`JmapEmailSubmissionCreate`].
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        submissions: BTreeMap<String, JmapEmailSubmissionCreate>,
    ) -> Result<Self, JmapEmailSubmissionSetError> {
        let account_id = session
            .primary_accounts
            .get(MAIL_CAPABILITY)
            .cloned()
            .unwrap_or_default();
        let api_url = &session.api_url;

        let args = serde_json::to_value(EmailSubmissionSetArgs {
            account_id,
            create: submissions,
        })
        .map_err(JmapEmailSubmissionSetError::SerializeArgs)?;

        let mut batch = JmapBatch::new();
        batch.add("EmailSubmission/set", args);
        let request = batch.into_request(vec![
            CORE_CAPABILITY.into(),
            MAIL_CAPABILITY.into(),
            SUBMISSION_CAPABILITY.into(),
        ]);

        Ok(Self {
            state: State::Send(JmapSend::new(http_auth, api_url, request)?),
        })
    }
}

impl JmapCoroutine for JmapEmailSubmissionSet {
    type Yield = JmapYield;
    type Return = Result<JmapEmailSubmissionSetOutput, JmapEmailSubmissionSetError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> JmapCoroutineState<Self::Yield, Self::Return> {
        trace!("EmailSubmission/set: {}", self.state);
        match &mut self.state {
            State::Send(send) => {
                let JmapSendOutput {
                    response,
                    keep_alive,
                } = jmap_try!(send, arg);

                let Some((name, args, _)) = response.method_responses.into_iter().next() else {
                    return JmapCoroutineState::Complete(Err(
                        JmapEmailSubmissionSetError::MissingResponse,
                    ));
                };

                if name == "error" {
                    let err = serde_json::from_value::<JmapMethodError>(args)
                        .unwrap_or(JmapMethodError::Unknown);
                    return JmapCoroutineState::Complete(Err(err.into()));
                }

                match serde_json::from_value::<EmailSubmissionSetResponse>(args) {
                    Ok(r) => JmapCoroutineState::Complete(Ok(JmapEmailSubmissionSetOutput {
                        new_state: r.new_state,
                        created: r.created.unwrap_or_default(),
                        not_created: r.not_created.unwrap_or_default(),
                        keep_alive,
                    })),
                    Err(err) => JmapCoroutineState::Complete(Err(
                        JmapEmailSubmissionSetError::ParseResponse(err),
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
struct EmailSubmissionSetArgs {
    account_id: String,
    create: BTreeMap<String, JmapEmailSubmissionCreate>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EmailSubmissionSetResponse {
    new_state: String,
    #[serde(default)]
    created: Option<BTreeMap<String, JmapEmailSubmission>>,
    #[serde(default)]
    not_created: Option<BTreeMap<String, JmapEmailSubmissionSetItemError>>,
}
