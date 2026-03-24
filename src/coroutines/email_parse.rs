//! I/O-free coroutine for the `Email/parse` method (RFC 8621 §4.11).

use std::collections::HashMap;

use io_stream::io::StreamIo;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    context::JmapContext,
    coroutines::send::{JmapBatch, SendJmapRequest, SendJmapRequestError, SendJmapRequestResult},
    types::{email::{Email, EmailProperty}, error::JmapMethodError, session::capabilities},
};

/// Errors that can occur during the coroutine progression.
#[derive(Debug, Error)]
pub enum ParseJmapEmailsError {
    #[error("Send JMAP request error: {0}")]
    Send(#[from] SendJmapRequestError),
    #[error("Serialize Email/parse args error: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    #[error("Parse Email/parse response error: {0}")]
    ParseResponse(#[source] serde_json::Error),
    #[error("Missing Email/parse response in method_responses")]
    MissingResponse,
    #[error("JMAP Email/parse method error: {0}")]
    Method(#[from] JmapMethodError),
}

/// Result returned by the [`ParseJmapEmails`] coroutine.
#[derive(Debug)]
pub enum ParseJmapEmailsResult {
    Ok {
        context: JmapContext,
        parsed: HashMap<String, Email>,
        not_parsable: Vec<String>,
        not_found: Vec<String>,
        keep_alive: bool,
    },
    Io(StreamIo),
    Err {
        context: JmapContext,
        err: ParseJmapEmailsError,
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
    parsed: HashMap<String, Email>,
    not_parsable: Option<Vec<String>>,
    not_found: Option<Vec<String>>,
}

/// I/O-free coroutine for the JMAP `Email/parse` method.
///
/// Parses RFC 5322 message blobs that are not yet stored as Email objects.
/// Useful for parsing attached `.eml` files.
pub struct ParseJmapEmails {
    send: SendJmapRequest,
}

impl ParseJmapEmails {
    /// Creates a new coroutine.
    ///
    /// - `blob_ids`: IDs of blobs to parse as RFC 5322 messages
    /// - `properties`: email properties to return (or `None` for all)
    pub fn new(
        context: JmapContext,
        blob_ids: Vec<String>,
        properties: Option<Vec<EmailProperty>>,
    ) -> Result<Self, ParseJmapEmailsError> {
        let account_id = context.account_id.clone().unwrap_or_default();
        let api_url = context
            .api_url()
            .cloned()
            .unwrap_or_else(|| "http://localhost".parse().unwrap());

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
            serde_json::to_value(&parse_args).map_err(ParseJmapEmailsError::SerializeArgs)?,
        );
        let request =
            batch.into_request(vec![capabilities::CORE.into(), capabilities::MAIL.into()]);

        Ok(Self {
            send: SendJmapRequest::new(context, &api_url, request)?,
        })
    }

    pub fn resume(&mut self, arg: Option<StreamIo>) -> ParseJmapEmailsResult {
        let (context, response, keep_alive) = match self.send.resume(arg) {
            SendJmapRequestResult::Ok { context, response, keep_alive } => {
                (context, response, keep_alive)
            }
            SendJmapRequestResult::Io(io) => return ParseJmapEmailsResult::Io(io),
            SendJmapRequestResult::Err { context, err } => {
                return ParseJmapEmailsResult::Err { context, err: err.into() }
            }
        };

        let Some((name, args, _)) = response.method_responses.into_iter().next() else {
            return ParseJmapEmailsResult::Err {
                context,
                err: ParseJmapEmailsError::MissingResponse,
            };
        };

        if name == "error" {
            let err = serde_json::from_value::<JmapMethodError>(args)
                .unwrap_or(JmapMethodError::Unknown);
            return ParseJmapEmailsResult::Err { context, err: err.into() };
        }

        match serde_json::from_value::<EmailParseResponse>(args) {
            Ok(r) => ParseJmapEmailsResult::Ok {
                context,
                parsed: r.parsed,
                not_parsable: r.not_parsable.unwrap_or_default(),
                not_found: r.not_found.unwrap_or_default(),
                keep_alive,
            },
            Err(err) => ParseJmapEmailsResult::Err {
                context,
                err: ParseJmapEmailsError::ParseResponse(err),
            },
        }
    }
}
