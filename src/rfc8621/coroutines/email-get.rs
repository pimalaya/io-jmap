//! I/O-free coroutine for the `Email/get` method (RFC 8621 §4.5).

use io_stream::io::StreamIo;
use secrecy::SecretString;
use serde::Serialize;
use thiserror::Error;

use crate::{
    rfc8620::coroutines::get::{JmapGet, JmapGetError, JmapGetResult},
    rfc8620::coroutines::send::{JmapBatch, JmapSend, JmapSendError},
    rfc8620::types::session::capabilities,
    rfc8620::types::session::JmapSession,
    rfc8621::types::email::Email,
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

/// Result returned by the [`JmapEmailGet`] coroutine.
#[derive(Debug)]
pub enum JmapEmailGetResult {
    Ok {
        emails: Vec<Email>,
        not_found: Vec<String>,
        new_state: String,
        keep_alive: bool,
    },
    Io {
        io: StreamIo,
    },
    Err {
        err: JmapEmailGetError,
    },
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
    #[serde(skip_serializing_if = "Option::is_none")]
    properties: Option<Vec<String>>,
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
    /// - `body_properties`: properties to include in body parts
    /// - `fetch_text_body_values`: whether to include `bodyValues` for text parts
    /// - `fetch_html_body_values`: whether to include `bodyValues` for HTML parts
    /// - `max_body_value_bytes`: max bytes per body value (0 = unlimited)
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        ids: Vec<String>,
        properties: Option<Vec<String>>,
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

    /// Makes the coroutine progress.
    pub fn resume(&mut self, arg: Option<StreamIo>) -> JmapEmailGetResult {
        match self.get.resume(arg) {
            JmapGetResult::Ok {
                list,
                not_found,
                state,
                keep_alive,
            } => JmapEmailGetResult::Ok {
                emails: list,
                not_found,
                new_state: state,
                keep_alive,
            },
            JmapGetResult::Io { io } => JmapEmailGetResult::Io { io },
            JmapGetResult::Err { err } => JmapEmailGetResult::Err { err: err.into() },
        }
    }
}
