//! I/O-free coroutine for the `Email/parse` method (RFC 8621 §4.11).

use alloc::{collections::BTreeMap, string::String, vec, vec::Vec};

use io_socket::io::{SocketInput, SocketOutput};
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    rfc8620::coroutines::send::{JmapBatch, JmapSend, JmapSendError, JmapSendResult},
    rfc8620::types::session::JmapSession,
    rfc8620::types::{error::JmapMethodError, session::capabilities},
    rfc8621::types::email::{Email, EmailProperty},
};

/// Errors that can occur during the coroutine progression.
#[derive(Debug, Error)]
pub enum JmapEmailParseError {
    #[error("Send JMAP request error: {0}")]
    Send(#[from] JmapSendError),
    #[error("Serialize Email/parse args error: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    #[error("Parse Email/parse response error: {0}")]
    ParseResponse(#[source] serde_json::Error),
    #[error("Missing Email/parse response in method_responses")]
    MissingResponse,
    #[error("JMAP Email/parse method error: {0}")]
    Method(#[from] JmapMethodError),
}

/// Result returned by the [`JmapEmailParse`] coroutine.
#[derive(Debug)]
pub enum JmapEmailParseResult {
    Ok {
        parsed: BTreeMap<String, Email>,
        not_parsable: Vec<String>,
        not_found: Vec<String>,
        keep_alive: bool,
    },
    Io {
        input: SocketInput,
    },
    Err {
        err: JmapEmailParseError,
    },
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EmailParseArgs<'a> {
    account_id: &'a str,
    blob_ids: &'a [String],
    #[serde(skip_serializing_if = "Option::is_none")]
    properties: Option<&'a [EmailProperty]>,
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
    parsed: BTreeMap<String, Email>,
    not_parsable: Option<Vec<String>>,
    not_found: Option<Vec<String>>,
}

/// I/O-free coroutine for the JMAP `Email/parse` method.
///
/// Parses RFC 5322 message blobs that are not yet stored as Email objects.
/// Useful for parsing attached `.eml` files.
pub struct JmapEmailParse {
    send: JmapSend,
}

impl JmapEmailParse {
    /// Creates a new coroutine.
    ///
    /// - `blob_ids`: IDs of blobs to parse as RFC 5322 messages
    /// - `properties`: email properties to return (or `None` for all)
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        blob_ids: Vec<String>,
        properties: Option<Vec<EmailProperty>>,
    ) -> Result<Self, JmapEmailParseError> {
        let account_id = session
            .primary_accounts
            .get(capabilities::MAIL)
            .cloned()
            .unwrap_or_default();
        let api_url = &session.api_url;

        let parse_args = EmailParseArgs {
            account_id: &account_id,
            blob_ids: &blob_ids,
            properties: properties.as_deref(),
            fetch_text_body_values: true,
            fetch_html_body_values: true,
            max_body_value_bytes: None,
        };

        let mut batch = JmapBatch::new();
        batch.add(
            "Email/parse",
            serde_json::to_value(&parse_args).map_err(JmapEmailParseError::SerializeArgs)?,
        );
        let request =
            batch.into_request(vec![capabilities::CORE.into(), capabilities::MAIL.into()]);

        Ok(Self {
            send: JmapSend::new(http_auth, api_url, request)?,
        })
    }

    pub fn resume(&mut self, arg: Option<SocketOutput>) -> JmapEmailParseResult {
        let (response, keep_alive) = match self.send.resume(arg) {
            JmapSendResult::Ok {
                response,
                keep_alive,
            } => (response, keep_alive),
            JmapSendResult::Io { input } => return JmapEmailParseResult::Io { input },
            JmapSendResult::Err { err } => return JmapEmailParseResult::Err { err: err.into() },
        };

        let Some((name, args, _)) = response.method_responses.into_iter().next() else {
            return JmapEmailParseResult::Err {
                err: JmapEmailParseError::MissingResponse,
            };
        };

        if name == "error" {
            let err =
                serde_json::from_value::<JmapMethodError>(args).unwrap_or(JmapMethodError::Unknown);
            return JmapEmailParseResult::Err { err: err.into() };
        }

        match serde_json::from_value::<EmailParseResponse>(args) {
            Ok(r) => JmapEmailParseResult::Ok {
                parsed: r.parsed,
                not_parsable: r.not_parsable.unwrap_or_default(),
                not_found: r.not_found.unwrap_or_default(),
                keep_alive,
            },
            Err(err) => JmapEmailParseResult::Err {
                err: JmapEmailParseError::ParseResponse(err),
            },
        }
    }
}
