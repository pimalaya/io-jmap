//! I/O-free coroutine for the `Email/get` method (RFC 8621 §4.5).

use alloc::{string::String, vec, vec::Vec};

use secrecy::SecretString;
use serde::Serialize;
use thiserror::Error;

use crate::coroutine::*;
use crate::{
    rfc8620::{get::*, send::*, session::JmapSession},
    rfc8621::{
        capabilities,
        email::{Email, EmailProperty},
    },
};

/// Errors that can occur during the coroutine progression.
#[derive(Debug, Error)]
pub enum JmapEmailGetError {
    #[error("Send JMAP request error: {0}")]
    Send(#[from] JmapSendError),
    #[error("Serialize Email/get args error: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    #[error("JMAP Email/get error: {0}")]
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

fn is_false(b: &bool) -> bool {
    !b
}

fn is_zero(v: &u64) -> bool {
    *v == 0
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EmailGetArgs {
    account_id: String,
    ids: Vec<String>,
    /// `EmailProperty`'s `Serialize` impl carries
    /// `rename_all = "camelCase"`, so each variant serializes as the
    /// JMAP wire string (`id`, `mailboxIds`, `sentAt`, ...). Callers
    /// pass the typed enum and serde handles the encoding.
    #[serde(skip_serializing_if = "Option::is_none")]
    properties: Option<Vec<EmailProperty>>,
    #[serde(skip_serializing_if = "is_false")]
    fetch_text_body_values: bool,
    #[serde(rename = "fetchHTMLBodyValues", skip_serializing_if = "is_false")]
    fetch_html_body_values: bool,
    #[serde(skip_serializing_if = "is_zero")]
    max_body_value_bytes: u64,
}

/// I/O-free coroutine for the JMAP `Email/get` method.
///
/// Fetches email objects by ID with the specified properties.
pub struct JmapEmailGet {
    get: JmapGet<Email>,
}

impl JmapEmailGet {
    /// Creates a new coroutine.
    ///
    /// - `ids`: email IDs to fetch
    /// - `properties`: specific properties to include, or `None` for all
    /// - `fetch_text_body_values`: whether to include `bodyValues` for text parts
    /// - `fetch_html_body_values`: whether to include `bodyValues` for HTML parts
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
            .get(capabilities::MAIL)
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
        let request =
            batch.into_request(vec![capabilities::CORE.into(), capabilities::MAIL.into()]);

        let send = JmapSend::new(http_auth, api_url, request)?;
        Ok(Self {
            get: JmapGet::from_send(send),
        })
    }
}

impl JmapCoroutine for JmapEmailGet {
    type Yield = JmapYield;
    type Return = Result<JmapEmailGetOutput, JmapEmailGetError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> JmapCoroutineState<Self::Yield, Self::Return> {
        match self.get.resume(arg) {
            JmapCoroutineState::Complete(Ok(JmapGetOutput {
                list,
                not_found,
                state,
                keep_alive,
            })) => JmapCoroutineState::Complete(Ok(JmapEmailGetOutput {
                emails: list,
                not_found,
                new_state: state,
                keep_alive,
            })),
            JmapCoroutineState::Complete(Err(err)) => JmapCoroutineState::Complete(Err(err.into())),
            JmapCoroutineState::Yielded(y) => JmapCoroutineState::Yielded(y),
        }
    }
}
