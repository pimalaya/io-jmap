//! Base I/O-free coroutine to send a JMAP API request.

use alloc::{collections::BTreeMap, format, string::String, vec::Vec};

use io_http::{
    coroutine::*,
    rfc9110::{
        request::HttpRequest,
        send::{HttpSendOutput, HttpSendYield},
    },
    rfc9112::send::{Http11Send, Http11SendError},
};
use log::trace;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

use crate::coroutine::*;

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

/// Successful terminal output of the [`JmapSend`] coroutine.
#[derive(Clone, Debug)]
pub struct JmapSendOutput {
    /// The parsed JMAP response.
    pub response: JmapResponse,
    /// Whether the server indicated the connection can be reused.
    pub keep_alive: bool,
}

/// I/O-free coroutine to send a JMAP API request and receive the response.
///
/// This is the base coroutine that all higher-level JMAP coroutines
/// delegate to. It wraps [`Http11Send`] and adds JSON serialization of
/// the request body and deserialization of the response body.
///
/// A 3xx response surfaces as
/// [`JmapSendError::UnexpectedRedirect`]; redirect-aware coroutines
/// ([`JmapSessionGet`](crate::rfc8620::session_get::JmapSessionGet),
/// [`JmapBlobDownload`](crate::rfc8620::blob_download::JmapBlobDownload),
/// [`JmapBlobUpload`](crate::rfc8620::blob_upload::JmapBlobUpload))
/// drive [`Http11Send`] directly and forward the redirect to the
/// caller.
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
}

impl JmapCoroutine for JmapSend {
    type Yield = JmapYield;
    type Return = Result<JmapSendOutput, JmapSendError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> JmapCoroutineState<Self::Yield, Self::Return> {
        match self.send.resume(arg) {
            HttpCoroutineState::Complete(Ok(HttpSendOutput {
                response,
                keep_alive,
                ..
            })) => {
                if !response.status.is_success() {
                    let err = JmapSendError::HttpStatus(*response.status);
                    return JmapCoroutineState::Complete(Err(err));
                }

                match serde_json::from_slice::<JmapResponse>(&response.body) {
                    Ok(response) => JmapCoroutineState::Complete(Ok(JmapSendOutput {
                        response,
                        keep_alive,
                    })),
                    Err(err) => {
                        JmapCoroutineState::Complete(Err(JmapSendError::ParseResponse(err)))
                    }
                }
            }
            HttpCoroutineState::Yielded(HttpSendYield::WantsRead) => {
                JmapCoroutineState::Yielded(JmapYield::WantsRead)
            }
            HttpCoroutineState::Yielded(HttpSendYield::WantsWrite(bytes)) => {
                JmapCoroutineState::Yielded(JmapYield::WantsWrite(bytes))
            }
            HttpCoroutineState::Yielded(HttpSendYield::WantsRedirect { .. }) => {
                JmapCoroutineState::Complete(Err(JmapSendError::UnexpectedRedirect))
            }
            HttpCoroutineState::Complete(Err(err)) => JmapCoroutineState::Complete(Err(err.into())),
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
