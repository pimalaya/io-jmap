//! JMAP `EmailSubmission/set` cancel coroutine (RFC 8621 §7.5): patches
//! `undoStatus: "canceled"` on each pending submission id.  Submissions not in
//! `pending` state surface in `notUpdated`.
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
//!     rfc8620::JmapSession,
//!     rfc8621::email_submission::cancel::JmapEmailSubmissionCancel,
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
//! let mut coroutine =
//!     JmapEmailSubmissionCancel::new(&session, &auth, vec!["s1".into()]).unwrap();
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

use core::fmt;

use alloc::{collections::BTreeMap, string::String, vec, vec::Vec};

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
            JmapEmailSubmission, JmapEmailSubmissionSetItemError, JmapEmailSubmissionUpdate,
            JmapUndoStatus, SUBMISSION_CAPABILITY,
        },
    },
};

/// Failure causes during a JMAP `EmailSubmission/set` cancel flow.
#[derive(Debug, Error)]
pub enum JmapEmailSubmissionCancelError {
    #[error("JMAP EmailSubmission/set (cancel) failed: missing response in method_responses")]
    MissingResponse,
    #[error("JMAP EmailSubmission/set (cancel) failed: {0}")]
    Send(#[from] JmapSendError),
    #[error("JMAP EmailSubmission/set (cancel) failed: serialize args: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    #[error("JMAP EmailSubmission/set (cancel) failed: parse response: {0}")]
    ParseResponse(#[source] serde_json::Error),
    #[error("JMAP EmailSubmission/set (cancel) failed: {0}")]
    Method(#[from] JmapMethodError),
}

/// Successful terminal output of [`JmapEmailSubmissionCancel`].
#[derive(Clone, Debug)]
pub struct JmapEmailSubmissionCancelOutput {
    pub new_state: String,
    pub updated: BTreeMap<String, Option<JmapEmailSubmission>>,
    pub not_updated: BTreeMap<String, JmapEmailSubmissionSetItemError>,
    pub keep_alive: bool,
}

/// I/O-free coroutine for canceling pending JMAP email submissions.
pub struct JmapEmailSubmissionCancel {
    state: State,
}

impl JmapEmailSubmissionCancel {
    /// `ids` is the list of submission IDs to cancel.
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        ids: Vec<String>,
    ) -> Result<Self, JmapEmailSubmissionCancelError> {
        let account_id = session
            .primary_accounts
            .get(MAIL_CAPABILITY)
            .cloned()
            .unwrap_or_default();
        let api_url = &session.api_url;

        let update = ids
            .into_iter()
            .map(|id| {
                (
                    id,
                    JmapEmailSubmissionUpdate {
                        undo_status: Some(JmapUndoStatus::Canceled),
                    },
                )
            })
            .collect();

        let args = serde_json::to_value(CancelEmailSubmissionsArgs { account_id, update })
            .map_err(JmapEmailSubmissionCancelError::SerializeArgs)?;

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

impl JmapCoroutine for JmapEmailSubmissionCancel {
    type Yield = JmapYield;
    type Return = Result<JmapEmailSubmissionCancelOutput, JmapEmailSubmissionCancelError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> JmapCoroutineState<Self::Yield, Self::Return> {
        trace!("JmapEmailSubmission/cancel: {}", self.state);
        match &mut self.state {
            State::Send(send) => {
                let JmapSendOutput {
                    response,
                    keep_alive,
                } = jmap_try!(send, arg);

                let Some((name, args, _)) = response.method_responses.into_iter().next() else {
                    return JmapCoroutineState::Complete(Err(
                        JmapEmailSubmissionCancelError::MissingResponse,
                    ));
                };

                if name == "error" {
                    let err = serde_json::from_value::<JmapMethodError>(args)
                        .unwrap_or(JmapMethodError::Unknown);
                    return JmapCoroutineState::Complete(Err(err.into()));
                }

                match serde_json::from_value::<EmailSubmissionCancelResponse>(args) {
                    Ok(r) => JmapCoroutineState::Complete(Ok(JmapEmailSubmissionCancelOutput {
                        new_state: r.new_state,
                        updated: r.updated.unwrap_or_default(),
                        not_updated: r.not_updated.unwrap_or_default(),
                        keep_alive,
                    })),
                    Err(err) => JmapCoroutineState::Complete(Err(
                        JmapEmailSubmissionCancelError::ParseResponse(err),
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
struct CancelEmailSubmissionsArgs {
    account_id: String,
    update: BTreeMap<String, JmapEmailSubmissionUpdate>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EmailSubmissionCancelResponse {
    new_state: String,
    #[serde(default)]
    updated: Option<BTreeMap<String, Option<JmapEmailSubmission>>>,
    #[serde(default)]
    not_updated: Option<BTreeMap<String, JmapEmailSubmissionSetItemError>>,
}
