//! JMAP `Email/get` coroutine (RFC 8621 §4.5): wraps the generic
//! [`JmapGet`] with the `Email/get`-specific args shape (property
//! selection, body-value fetch toggles) and a typed [`Email`] decoder.
//!
//! # Example
//!
//! ```rust,no_run
//! use io_jmap::{
//!     rfc8620::JmapSession,
//!     rfc8621::email::get::JmapEmailGet,
//! };
//! use secrecy::SecretString;
//!
//! # fn demo(session: &JmapSession) {
//! let auth = SecretString::from("Bearer xyz");
//! let coroutine = JmapEmailGet::new(
//!     session,
//!     &auth,
//!     vec!["e1".into()],
//!     None,
//!     true,
//!     false,
//!     0,
//! )
//! .unwrap();
//! # let _ = coroutine;
//! # }
//! ```

use core::fmt;

use alloc::{string::String, vec, vec::Vec};

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
        email::{Email, EmailProperty},
    },
};

/// Failure causes during a JMAP `Email/get` flow.
#[derive(Debug, Error)]
pub enum JmapEmailGetError {
    #[error("JMAP Email/get failed: {0}")]
    Send(#[from] JmapSendError),
    #[error("JMAP Email/get failed: serialize args: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    #[error("JMAP Email/get failed: {0}")]
    Get(#[from] JmapGetError),
}

/// Successful terminal output of [`JmapEmailGet`].
#[derive(Clone, Debug)]
pub struct JmapEmailGetOutput {
    pub emails: Vec<Email>,
    pub not_found: Vec<String>,
    pub new_state: String,
    pub keep_alive: bool,
}

/// I/O-free coroutine for the JMAP `Email/get` method.
pub struct JmapEmailGet {
    state: State,
}

impl JmapEmailGet {
    /// - `ids`: email IDs to fetch
    /// - `properties`: specific properties to include, or `None` for all
    /// - `fetch_text_body_values`: whether to include `bodyValues` for
    ///   text parts
    /// - `fetch_html_body_values`: whether to include `bodyValues` for
    ///   HTML parts
    /// - `max_body_value_bytes`: max bytes per body value (0 = unlimited)
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        ids: Vec<String>,
        properties: Option<Vec<EmailProperty>>,
        fetch_text_body_values: bool,
        fetch_html_body_values: bool,
        max_body_value_bytes: u64,
    ) -> Result<Self, JmapEmailGetError> {
        let account_id = session
            .primary_accounts
            .get(MAIL_CAPABILITY)
            .cloned()
            .unwrap_or_default();
        let api_url = &session.api_url;

        let args = serde_json::to_value(EmailGetArgs {
            account_id,
            ids,
            properties,
            fetch_text_body_values,
            fetch_html_body_values,
            max_body_value_bytes,
        })
        .map_err(JmapEmailGetError::SerializeArgs)?;

        let mut batch = JmapBatch::new();
        batch.add("Email/get", args);
        let request = batch.into_request(vec![CORE_CAPABILITY.into(), MAIL_CAPABILITY.into()]);

        let send = JmapSend::new(http_auth, api_url, request)?;
        Ok(Self {
            state: State::Get(JmapGet::from_send(send)),
        })
    }
}

impl JmapCoroutine for JmapEmailGet {
    type Yield = JmapYield;
    type Return = Result<JmapEmailGetOutput, JmapEmailGetError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> JmapCoroutineState<Self::Yield, Self::Return> {
        trace!("Email/get: {}", self.state);
        match &mut self.state {
            State::Get(get) => {
                let JmapGetOutput {
                    list,
                    not_found,
                    state,
                    keep_alive,
                } = jmap_try!(get, arg);
                JmapCoroutineState::Complete(Ok(JmapEmailGetOutput {
                    emails: list,
                    not_found,
                    new_state: state,
                    keep_alive,
                }))
            }
        }
    }
}

enum State {
    Get(JmapGet<Email>),
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
struct EmailGetArgs {
    account_id: String,
    ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    properties: Option<Vec<EmailProperty>>,
    #[serde(skip_serializing_if = "is_false")]
    fetch_text_body_values: bool,
    #[serde(rename = "fetchHTMLBodyValues", skip_serializing_if = "is_false")]
    fetch_html_body_values: bool,
    #[serde(skip_serializing_if = "is_zero")]
    max_body_value_bytes: u64,
}

fn is_false(b: &bool) -> bool {
    !b
}

fn is_zero(v: &u64) -> bool {
    *v == 0
}
