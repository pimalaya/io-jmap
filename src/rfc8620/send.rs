//! Base I/O-free coroutine to send a JMAP API request.

use alloc::{collections::BTreeMap, format, string::String, vec::Vec};

use io_http::{
    rfc9110::request::HttpRequest,
    rfc9112::send::{Http11Send, Http11SendError, Http11SendResult},
};
use log::trace;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

/// The JMAP Request object (RFC 8620 §3.3).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapRequest {
    /// Capability URNs required by the methods in this request.
    pub using: Vec<String>,

    /// The method calls to execute, as `(methodName, args, callId)` tuples.
    pub method_calls: Vec<(String, serde_json::Value, String)>,

    /// Client-assigned IDs for newly created objects.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_ids: Option<BTreeMap<String, String>>,
}

/// The JMAP Response object (RFC 8620 §3.4).
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapResponse {
    /// Method responses in `(methodName, result, callId)` format.
    ///
    /// If a method failed, `methodName` is `"error"` and `result`
    /// is a [`JmapMethodError`] object.
    ///
    /// [`JmapMethodError`]: crate::rfc8620::error::JmapMethodError
    pub method_responses: Vec<(String, serde_json::Value, String)>,

    /// Server-assigned IDs for objects created by this request.
    #[serde(default)]
    pub created_ids: Option<BTreeMap<String, String>>,

    /// The current state of the session after this request.
    pub session_state: String,
}

/// Errors that can occur during the coroutine progression.
#[derive(Debug, Error)]
pub enum JmapSendError {
    #[error("Send HTTP request error: {0}")]
    SendHttp(#[from] Http11SendError),
    #[error("Serialize JMAP request error: {0}")]
    SerializeRequest(#[source] serde_json::Error),
    #[error("Parse JMAP response error: {0}")]
    ParseResponse(#[source] serde_json::Error),
    #[error("JMAP server returned HTTP {0}")]
    HttpStatus(u16),
    #[error("JMAP server returned unexpected redirect")]
    UnexpectedRedirect,
}

/// Result returned by the [`JmapSend`] coroutine.
#[derive(Debug)]
pub enum JmapSendResult {
    /// The coroutine has successfully completed.
    Ok {
        response: JmapResponse,
        keep_alive: bool,
    },
    /// The coroutine needs more bytes to be read from the socket.
    WantsRead,
    /// The coroutine wants the given bytes to be written to the socket.
    WantsWrite(Vec<u8>),
    /// The coroutine encountered an error.
    Err(JmapSendError),
}

/// I/O-free coroutine to send a JMAP API request and receive the response.
///
/// This is the base coroutine that all higher-level JMAP coroutines
/// delegate to. It wraps [`Http11Send`] and adds JSON serialization of
/// the request body and deserialization of the response body.
///
/// The caller drives the coroutine by calling [`resume`] in a loop
/// and handling the returned [`JmapSendResult`].
///
/// [`resume`]: JmapSend::resume
pub struct JmapSend {
    send: Http11Send,
}

impl JmapSend {
    /// Creates a new JMAP request coroutine.
    ///
    /// Serializes `request` as JSON and builds an HTTP POST to `api_url`
    /// with the bearer token from `http_auth`.
    pub fn new(
        http_auth: &SecretString,
        api_url: &Url,
        request: JmapRequest,
    ) -> Result<Self, JmapSendError> {
        let body = serde_json::to_vec(&request).map_err(JmapSendError::SerializeRequest)?;

        let host = api_url.host_str().unwrap_or("localhost");

        let mut http_request = HttpRequest::get(api_url.clone())
            .header("Host", host)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .header("Authorization", http_auth.expose_secret())
            .body(body);

        http_request.method = "POST".into();

        trace!("send JMAP request to {api_url}");

        Ok(Self {
            send: Http11Send::new(http_request),
        })
    }

    /// Advances the coroutine.
    ///
    /// Pass [`None`] when there is no data to provide (initial call,
    /// after a write). Pass `Some(data)` with bytes read from the
    /// socket after a [`JmapSendResult::WantsRead`]. Pass `Some(&[])`
    /// to signal EOF.
    pub fn resume(&mut self, arg: Option<&[u8]>) -> JmapSendResult {
        match self.send.resume(arg) {
            Http11SendResult::Ok {
                response,
                keep_alive,
                ..
            } => {
                if !response.status.is_success() {
                    let err = JmapSendError::HttpStatus(*response.status);
                    return JmapSendResult::Err(err);
                }

                match serde_json::from_slice::<JmapResponse>(&response.body) {
                    Ok(jmap_response) => JmapSendResult::Ok {
                        response: jmap_response,
                        keep_alive,
                    },
                    Err(err) => JmapSendResult::Err(JmapSendError::ParseResponse(err)),
                }
            }
            Http11SendResult::WantsRead => JmapSendResult::WantsRead,
            Http11SendResult::WantsWrite(bytes) => JmapSendResult::WantsWrite(bytes),
            Http11SendResult::WantsRedirect { .. } => {
                JmapSendResult::Err(JmapSendError::UnexpectedRedirect)
            }
            Http11SendResult::Err(err) => JmapSendResult::Err(err.into()),
        }
    }
}

/// Builder for batched JMAP requests.
///
/// JMAP allows multiple method calls in a single HTTP request. This
/// builder generates call IDs and supports back-references via the
/// JMAP Result Reference (RFC 8620 §7.1).
///
/// # Example
///
/// ```rust,ignore
/// let mut batch = JmapBatch::new();
/// let query_id = batch.add("Email/query", json!({ "accountId": "...", "filter": {} }));
/// batch.add("Email/get", json!({
///     "accountId": "...",
///     "#ids": {
///         "resultOf": query_id,
///         "name": "Email/query",
///         "path": "/ids"
///     }
/// }));
/// let request = batch.into_request(vec!["urn:ietf:params:jmap:core".into(),
///                                       "urn:ietf:params:jmap:mail".into()]);
/// ```
#[derive(Debug, Default)]
pub struct JmapBatch {
    calls: Vec<(String, serde_json::Value, String)>,
    counter: usize,
}

impl JmapBatch {
    /// Creates a new empty batch.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a method call to the batch.
    ///
    /// Returns the call ID (`"c0"`, `"c1"`, …) for use in
    /// back-references from later calls.
    pub fn add(&mut self, method: impl Into<String>, args: serde_json::Value) -> String {
        let call_id = format!("c{}", self.counter);
        self.counter += 1;
        self.calls.push((method.into(), args, call_id.clone()));
        call_id
    }

    /// Consumes the batch and returns a [`JmapRequest`].
    pub fn into_request(self, using: Vec<String>) -> JmapRequest {
        JmapRequest {
            using,
            method_calls: self.calls,
            created_ids: None,
        }
    }
}
