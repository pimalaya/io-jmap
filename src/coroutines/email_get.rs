//! I/O-free coroutine for the `Email/get` method (RFC 8621 §4.5).

use io_stream::io::StreamIo;
use serde::Deserialize;
use thiserror::Error;

use crate::{
    context::JmapContext,
    coroutines::send::{JmapBatch, SendJmapRequest, SendJmapRequestError, SendJmapRequestResult},
    types::{email::Email, error::JmapMethodError, session::capabilities},
};

/// Errors that can occur during the coroutine progression.
#[derive(Debug, Error)]
pub enum GetJmapEmailsError {
    #[error("Send JMAP request error: {0}")]
    Send(#[from] SendJmapRequestError),
    #[error("Parse Email/get response error: {0}")]
    ParseResponse(#[source] serde_json::Error),
    #[error("Missing Email/get response in method_responses")]
    MissingResponse,
    #[error("JMAP Email/get method error: {0}")]
    Method(#[from] JmapMethodError),
}

/// Result returned by the [`GetJmapEmails`] coroutine.
#[derive(Debug)]
pub enum GetJmapEmailsResult {
    Ok {
        context: JmapContext,
        emails: Vec<Email>,
        not_found: Vec<String>,
        new_state: String,
        keep_alive: bool,
    },
    Io(StreamIo),
    Err {
        context: JmapContext,
        err: GetJmapEmailsError,
    },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EmailGetResponse {
    list: Vec<Email>,
    #[serde(default)]
    not_found: Vec<String>,
    state: String,
}

/// I/O-free coroutine for the JMAP `Email/get` method.
///
/// Fetches email objects by ID with the specified properties.
pub struct GetJmapEmails {
    send: SendJmapRequest,
}

impl GetJmapEmails {
    /// Creates a new coroutine.
    ///
    /// - `ids`: email IDs to fetch
    /// - `properties`: specific properties to include, or `None` for all
    /// - `body_properties`: properties to include in body parts
    /// - `fetch_text_body_values`: whether to include `bodyValues` for text parts
    /// - `fetch_html_body_values`: whether to include `bodyValues` for HTML parts
    /// - `max_body_value_bytes`: max bytes per body value (0 = unlimited)
    pub fn new(
        context: JmapContext,
        ids: Vec<String>,
        properties: Option<Vec<String>>,
        fetch_text_body_values: bool,
        fetch_html_body_values: bool,
        max_body_value_bytes: Option<u64>,
    ) -> Result<Self, GetJmapEmailsError> {
        let account_id = context.account_id.clone().unwrap_or_default();
        let api_url = context
            .api_url()
            .cloned()
            .unwrap_or_else(|| "http://localhost".parse().unwrap());

        let mut args = serde_json::json!({
            "accountId": account_id,
            "ids": ids
        });

        if let Some(props) = properties {
            args["properties"] = serde_json::json!(props);
        }

        if fetch_text_body_values {
            args["fetchTextBodyValues"] = serde_json::json!(true);
        }

        if fetch_html_body_values {
            args["fetchHTMLBodyValues"] = serde_json::json!(true);
        }

        if let Some(max) = max_body_value_bytes {
            if max > 0 {
                args["maxBodyValueBytes"] = serde_json::json!(max);
            }
        }

        let mut batch = JmapBatch::new();
        batch.add("Email/get", args);
        let request = batch.into_request(vec![
            capabilities::CORE.into(),
            capabilities::MAIL.into(),
        ]);

        Ok(Self {
            send: SendJmapRequest::new(context, &api_url, request)?,
        })
    }

    /// Makes the coroutine progress.
    pub fn resume(&mut self, arg: Option<StreamIo>) -> GetJmapEmailsResult {
        let (context, response, keep_alive) = match self.send.resume(arg) {
            SendJmapRequestResult::Ok { context, response, keep_alive } => {
                (context, response, keep_alive)
            }
            SendJmapRequestResult::Io(io) => return GetJmapEmailsResult::Io(io),
            SendJmapRequestResult::Err { context, err } => {
                return GetJmapEmailsResult::Err {
                    context,
                    err: err.into(),
                }
            }
        };

        let Some((name, args, _)) = response.method_responses.into_iter().next() else {
            return GetJmapEmailsResult::Err {
                context,
                err: GetJmapEmailsError::MissingResponse,
            };
        };

        if name == "error" {
            let err = serde_json::from_value::<JmapMethodError>(args)
                .unwrap_or(JmapMethodError::Unknown);
            return GetJmapEmailsResult::Err {
                context,
                err: err.into(),
            };
        }

        match serde_json::from_value::<EmailGetResponse>(args) {
            Ok(r) => GetJmapEmailsResult::Ok {
                context,
                emails: r.list,
                not_found: r.not_found,
                new_state: r.state,
                keep_alive,
            },
            Err(err) => GetJmapEmailsResult::Err {
                context,
                err: GetJmapEmailsError::ParseResponse(err),
            },
        }
    }
}
