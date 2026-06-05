//! JMAP `Email/copy` coroutine (RFC 8621 §4.10): copies emails from
//! one account into the current session account.
//!
//! # Example
//!
//! ```rust,no_run
//! use std::collections::BTreeMap;
//!
//! use io_jmap::{
//!     rfc8620::JmapSession,
//!     rfc8621::email::{EmailCopy, copy::JmapEmailCopy},
//! };
//! use secrecy::SecretString;
//!
//! # fn demo(session: &JmapSession) {
//! let auth = SecretString::from("Bearer xyz");
//! let mut create = BTreeMap::new();
//! create.insert(
//!     "c1".to_string(),
//!     EmailCopy {
//!         id: "e1".into(),
//!         mailbox_ids: Default::default(),
//!         keywords: None,
//!         received_at: None,
//!     },
//! );
//! let coroutine = JmapEmailCopy::new(session, &auth, "from", create).unwrap();
//! # let _ = coroutine;
//! # }
//! ```

use core::fmt;

use alloc::{collections::BTreeMap, string::String, vec};

use log::trace;
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::coroutine::*;
use crate::jmap_try;
use crate::{
    rfc8620::{CORE_CAPABILITY, JmapBatch, JmapMethodError, JmapSession, send::*},
    rfc8621::{
        MAIL_CAPABILITY,
        email::{Email, EmailCopy, EmailCopyError},
    },
};

/// Failure causes during a JMAP `Email/copy` flow.
#[derive(Debug, Error)]
pub enum JmapEmailCopyError {
    #[error("JMAP Email/copy failed: missing response in method_responses")]
    MissingResponse,
    #[error("JMAP Email/copy failed: {0}")]
    Send(#[from] JmapSendError),
    #[error("JMAP Email/copy failed: serialize args: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    #[error("JMAP Email/copy failed: parse response: {0}")]
    ParseResponse(#[source] serde_json::Error),
    #[error("JMAP Email/copy failed: {0}")]
    Method(#[from] JmapMethodError),
}

/// Successful terminal output of [`JmapEmailCopy`].
#[derive(Clone, Debug)]
pub struct JmapEmailCopyOutput {
    pub new_state: String,
    pub created: BTreeMap<String, Email>,
    pub not_created: BTreeMap<String, EmailCopyError>,
    pub keep_alive: bool,
}

/// I/O-free coroutine for the JMAP `Email/copy` method.
pub struct JmapEmailCopy {
    state: State,
}

impl JmapEmailCopy {
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        from_account_id: impl Into<String>,
        emails: BTreeMap<String, EmailCopy>,
    ) -> Result<Self, JmapEmailCopyError> {
        let account_id = session
            .primary_accounts
            .get(MAIL_CAPABILITY)
            .cloned()
            .unwrap_or_default();
        let api_url = &session.api_url;

        let args = serde_json::to_value(EmailCopyArgs {
            from_account_id: from_account_id.into(),
            account_id,
            create: emails,
        })
        .map_err(JmapEmailCopyError::SerializeArgs)?;

        let mut batch = JmapBatch::new();
        batch.add("Email/copy", args);
        let request = batch.into_request(vec![CORE_CAPABILITY.into(), MAIL_CAPABILITY.into()]);

        Ok(Self {
            state: State::Send(JmapSend::new(http_auth, api_url, request)?),
        })
    }
}

impl JmapCoroutine for JmapEmailCopy {
    type Yield = JmapYield;
    type Return = Result<JmapEmailCopyOutput, JmapEmailCopyError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> JmapCoroutineState<Self::Yield, Self::Return> {
        trace!("Email/copy: {}", self.state);
        match &mut self.state {
            State::Send(send) => {
                let JmapSendOutput {
                    response,
                    keep_alive,
                } = jmap_try!(send, arg);

                let Some((name, args, _)) = response.method_responses.into_iter().next() else {
                    return JmapCoroutineState::Complete(Err(JmapEmailCopyError::MissingResponse));
                };

                if name == "error" {
                    let err = serde_json::from_value::<JmapMethodError>(args)
                        .unwrap_or(JmapMethodError::Unknown);
                    return JmapCoroutineState::Complete(Err(err.into()));
                }

                match serde_json::from_value::<EmailCopyResponse>(args) {
                    Ok(r) => JmapCoroutineState::Complete(Ok(JmapEmailCopyOutput {
                        new_state: r.new_state,
                        created: r.created,
                        not_created: r.not_created,
                        keep_alive,
                    })),
                    Err(err) => {
                        JmapCoroutineState::Complete(Err(JmapEmailCopyError::ParseResponse(err)))
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
struct EmailCopyArgs {
    from_account_id: String,
    account_id: String,
    create: BTreeMap<String, EmailCopy>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EmailCopyResponse {
    new_state: String,
    #[serde(default)]
    created: BTreeMap<String, Email>,
    #[serde(default)]
    not_created: BTreeMap<String, EmailCopyError>,
}
