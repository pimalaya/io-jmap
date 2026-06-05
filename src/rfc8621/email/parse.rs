//! JMAP `Email/parse` coroutine (RFC 8621 §4.11): parses RFC 5322 message blobs
//! that are not yet stored as Email objects (useful for attached `.eml` files).
//!
//! # Example
//!
//! ```rust,no_run
//! use io_jmap::{
//!     rfc8620::JmapSession,
//!     rfc8621::email::parse::{JmapEmailParse, JmapEmailParseOptions},
//! };
//! use secrecy::SecretString;
//!
//! # fn demo(session: &JmapSession) {
//! let auth = SecretString::from("Bearer xyz");
//! let coroutine = JmapEmailParse::new(
//!     session,
//!     &auth,
//!     vec!["b1".into()],
//!     JmapEmailParseOptions::default(),
//! )
//! .unwrap();
//! # let _ = coroutine;
//! # }
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
        email::{JmapEmail, JmapEmailProperty},
    },
};

/// Failure causes during a JMAP `Email/parse` flow.
#[derive(Debug, Error)]
pub enum JmapEmailParseError {
    #[error("JMAP Email/parse failed: missing response in method_responses")]
    MissingResponse,
    #[error("JMAP Email/parse failed: {0}")]
    Send(#[from] JmapSendError),
    #[error("JMAP Email/parse failed: serialize args: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    #[error("JMAP Email/parse failed: parse response: {0}")]
    ParseResponse(#[source] serde_json::Error),
    #[error("JMAP Email/parse failed: {0}")]
    Method(#[from] JmapMethodError),
}

/// Options for [`JmapEmailParse::new`].
#[derive(Clone, Debug, Default)]
pub struct JmapEmailParseOptions {
    /// Email properties to return; `None` returns all.
    pub properties: Option<Vec<JmapEmailProperty>>,
}

/// Successful terminal output of [`JmapEmailParse`].
#[derive(Clone, Debug)]
pub struct JmapEmailParseOutput {
    pub parsed: BTreeMap<String, JmapEmail>,
    pub not_parsable: Vec<String>,
    pub not_found: Vec<String>,
    pub keep_alive: bool,
}

/// I/O-free coroutine for the JMAP `Email/parse` method.
pub struct JmapEmailParse {
    state: State,
}

impl JmapEmailParse {
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        blob_ids: Vec<String>,
        opts: JmapEmailParseOptions,
    ) -> Result<Self, JmapEmailParseError> {
        let account_id = session
            .primary_accounts
            .get(MAIL_CAPABILITY)
            .cloned()
            .unwrap_or_default();
        let api_url = &session.api_url;

        let parse_args = EmailParseArgs {
            account_id: &account_id,
            blob_ids: &blob_ids,
            properties: opts.properties.as_deref(),
            fetch_text_body_values: true,
            fetch_html_body_values: true,
            max_body_value_bytes: None,
        };

        let mut batch = JmapBatch::new();
        batch.add(
            "Email/parse",
            serde_json::to_value(&parse_args).map_err(JmapEmailParseError::SerializeArgs)?,
        );
        let request = batch.into_request(vec![CORE_CAPABILITY.into(), MAIL_CAPABILITY.into()]);

        Ok(Self {
            state: State::Send(JmapSend::new(http_auth, api_url, request)?),
        })
    }
}

impl JmapCoroutine for JmapEmailParse {
    type Yield = JmapYield;
    type Return = Result<JmapEmailParseOutput, JmapEmailParseError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> JmapCoroutineState<Self::Yield, Self::Return> {
        trace!("Email/parse: {}", self.state);
        match &mut self.state {
            State::Send(send) => {
                let JmapSendOutput {
                    response,
                    keep_alive,
                } = jmap_try!(send, arg);

                let Some((name, args, _)) = response.method_responses.into_iter().next() else {
                    return JmapCoroutineState::Complete(Err(JmapEmailParseError::MissingResponse));
                };

                if name == "error" {
                    let err = serde_json::from_value::<JmapMethodError>(args)
                        .unwrap_or(JmapMethodError::Unknown);
                    return JmapCoroutineState::Complete(Err(err.into()));
                }

                match serde_json::from_value::<EmailParseResponse>(args) {
                    Ok(r) => JmapCoroutineState::Complete(Ok(JmapEmailParseOutput {
                        parsed: r.parsed,
                        not_parsable: r.not_parsable.unwrap_or_default(),
                        not_found: r.not_found.unwrap_or_default(),
                        keep_alive,
                    })),
                    Err(err) => {
                        JmapCoroutineState::Complete(Err(JmapEmailParseError::ParseResponse(err)))
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
struct EmailParseArgs<'a> {
    account_id: &'a str,
    blob_ids: &'a [String],
    #[serde(skip_serializing_if = "Option::is_none")]
    properties: Option<&'a [JmapEmailProperty]>,
    fetch_text_body_values: bool,
    #[serde(rename = "fetchHTMLBodyValues")]
    fetch_html_body_values: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_body_value_bytes: Option<u64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EmailParseResponse {
    #[serde(default)]
    parsed: BTreeMap<String, JmapEmail>,
    not_parsable: Option<Vec<String>>,
    not_found: Option<Vec<String>>,
}
