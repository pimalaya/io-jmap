//! JMAP `Email/import` coroutine (RFC 8621 §4.9): imports RFC 5322 messages
//! (previously uploaded as blobs) into mailboxes. JMAP equivalent of IMAP
//! `APPEND`.
//!
//! # Example
//!
//! ```rust,no_run
//! use std::collections::BTreeMap;
//!
//! use io_jmap::{
//!     rfc8620::JmapSession,
//!     rfc8621::email::{JmapEmailImportArgs, import::JmapEmailImport},
//! };
//! use secrecy::SecretString;
//!
//! # fn demo(session: &JmapSession) {
//! let auth = SecretString::from("Bearer xyz");
//! let mut emails = BTreeMap::new();
//! emails.insert(
//!     "c1".to_string(),
//!     JmapEmailImportArgs {
//!         blob_id: "b1".into(),
//!         mailbox_ids: Default::default(),
//!         keywords: None,
//!         received_at: None,
//!     },
//! );
//! let coroutine = JmapEmailImport::new(session, &auth, emails).unwrap();
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
        email::{JmapEmail, JmapEmailImportArgs, JmapEmailImportItemError},
    },
};

/// Failure causes during a JMAP `Email/import` flow.
#[derive(Debug, Error)]
pub enum JmapEmailImportError {
    #[error("JMAP Email/import failed: missing response in method_responses")]
    MissingResponse,
    #[error("JMAP Email/import failed: {0}")]
    Send(#[from] JmapSendError),
    #[error("JMAP Email/import failed: serialize args: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    #[error("JMAP Email/import failed: parse response: {0}")]
    ParseResponse(#[source] serde_json::Error),
    #[error("JMAP Email/import failed: {0}")]
    Method(#[from] JmapMethodError),
}

/// Successful terminal output of [`JmapEmailImport`].
#[derive(Clone, Debug)]
pub struct JmapEmailImportOutput {
    pub new_state: String,
    pub created: BTreeMap<String, JmapEmail>,
    pub not_created: BTreeMap<String, JmapEmailImportItemError>,
    pub keep_alive: bool,
}

/// I/O-free coroutine for the JMAP `Email/import` method.
pub struct JmapEmailImport {
    state: State,
}

impl JmapEmailImport {
    /// `emails` maps client-assigned IDs to [`JmapEmailImportArgs`] descriptors.
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        emails: BTreeMap<String, JmapEmailImportArgs>,
    ) -> Result<Self, JmapEmailImportError> {
        let account_id = session
            .primary_accounts
            .get(MAIL_CAPABILITY)
            .cloned()
            .unwrap_or_default();
        let api_url = &session.api_url;

        let args = serde_json::to_value(EmailImportArgs { account_id, emails })
            .map_err(JmapEmailImportError::SerializeArgs)?;

        let mut batch = JmapBatch::new();
        batch.add("Email/import", args);
        let request = batch.into_request(vec![CORE_CAPABILITY.into(), MAIL_CAPABILITY.into()]);

        Ok(Self {
            state: State::Send(JmapSend::new(http_auth, api_url, request)?),
        })
    }
}

impl JmapCoroutine for JmapEmailImport {
    type Yield = JmapYield;
    type Return = Result<JmapEmailImportOutput, JmapEmailImportError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> JmapCoroutineState<Self::Yield, Self::Return> {
        trace!("Email/import: {}", self.state);
        match &mut self.state {
            State::Send(send) => {
                let JmapSendOutput {
                    response,
                    keep_alive,
                } = jmap_try!(send, arg);

                let Some((name, args, _)) = response.method_responses.into_iter().next() else {
                    return JmapCoroutineState::Complete(Err(
                        JmapEmailImportError::MissingResponse,
                    ));
                };

                if name == "error" {
                    let err = serde_json::from_value::<JmapMethodError>(args)
                        .unwrap_or(JmapMethodError::Unknown);
                    return JmapCoroutineState::Complete(Err(err.into()));
                }

                match serde_json::from_value::<EmailImportResponse>(args) {
                    Ok(r) => JmapCoroutineState::Complete(Ok(JmapEmailImportOutput {
                        new_state: r.new_state,
                        created: r.created,
                        not_created: r.not_created,
                        keep_alive,
                    })),
                    Err(err) => {
                        JmapCoroutineState::Complete(Err(JmapEmailImportError::ParseResponse(err)))
                    }
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
struct EmailImportArgs {
    account_id: String,
    emails: BTreeMap<String, JmapEmailImportArgs>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EmailImportResponse {
    new_state: String,
    #[serde(default)]
    created: BTreeMap<String, JmapEmail>,
    #[serde(default)]
    not_created: BTreeMap<String, JmapEmailImportItemError>,
}
