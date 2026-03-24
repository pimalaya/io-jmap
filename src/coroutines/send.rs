//! Base I/O-free coroutine to send a JMAP API request.

use std::collections::HashMap;

use http::{
    header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE},
    Method,
};
use io_http::v1_1::coroutines::send::{SendHttp, SendHttpError, SendHttpResult};
use io_stream::io::StreamIo;
use log::info;
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

use crate::context::JmapContext;

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
    pub created_ids: Option<HashMap<String, String>>,
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
    /// [`JmapMethodError`]: crate::types::error::JmapMethodError
    pub method_responses: Vec<(String, serde_json::Value, String)>,

    /// Server-assigned IDs for objects created by this request.
    #[serde(default)]
    pub created_ids: Option<HashMap<String, String>>,

    /// The current state of the session after this request.
    pub session_state: String,
}

/// Errors that can occur during the coroutine progression.
#[derive(Debug, Error)]
pub enum SendJmapRequestError {
    #[error("Build HTTP request error: {0}")]
    BuildHttp(#[from] http::Error),
    #[error("Send HTTP request error: {0}")]
    SendHttp(#[from] SendHttpError),
    #[error("Serialize JMAP request error: {0}")]
    SerializeRequest(#[source] serde_json::Error),
    #[error("Parse JMAP response error: {0}")]
    ParseResponse(#[source] serde_json::Error),
    #[error("JMAP server returned HTTP {0}")]
    HttpStatus(u16),
}

/// Result returned by the [`SendJmapRequest`] coroutine.
#[derive(Debug)]
pub enum SendJmapRequestResult {
    /// The coroutine has successfully completed.
    Ok {
        context: JmapContext,
        response: JmapResponse,
        keep_alive: bool,
    },
    /// The coroutine wants stream I/O.
    Io(StreamIo),
    /// The coroutine encountered an error.
    Err {
        context: JmapContext,
        err: SendJmapRequestError,
    },
}

/// I/O-free coroutine to send a JMAP API request and receive the response.
///
/// This is the base coroutine that all higher-level JMAP coroutines
/// delegate to. It wraps [`SendHttp`] and adds JSON serialization of
/// the request body and deserialization of the response body.
///
/// The caller drives the coroutine by calling [`resume`] in a loop
/// and handling the returned [`SendJmapRequestResult`].
///
/// [`resume`]: SendJmapRequest::resume
pub struct SendJmapRequest {
    context: Option<JmapContext>,
    send: SendHttp,
}

impl SendJmapRequest {
    /// Creates a new JMAP request coroutine.
    ///
    /// Serializes `request` as JSON and builds an HTTP POST to `api_url`
    /// with the bearer token from `context`.
    pub fn new(
        context: JmapContext,
        api_url: &Url,
        request: JmapRequest,
    ) -> Result<Self, SendJmapRequestError> {
        let body = serde_json::to_vec(&request).map_err(SendJmapRequestError::SerializeRequest)?;

        let host = api_url.host_str().unwrap_or("localhost");
        let path = api_url.path();

        let mut builder = http::Request::builder()
            .method(Method::POST)
            .uri(path)
            .header("Host", host)
            .header(CONTENT_TYPE, "application/json")
            .header(ACCEPT, "application/json");

        if let Some(auth) = &context.http_auth {
            let auth = auth.expose_secret();
            builder = builder.header(AUTHORIZATION, auth);
        }

        let http_request = builder.body(body)?;

        info!("send JMAP request to {api_url}");

        Ok(Self {
            context: Some(context),
            send: SendHttp::new(http_request),
        })
    }

    /// Makes the coroutine progress.
    pub fn resume(&mut self, arg: Option<StreamIo>) -> SendJmapRequestResult {
        let ok = match self.send.resume(arg) {
            SendHttpResult::Ok(ok) => ok,
            SendHttpResult::Io(io) => return SendJmapRequestResult::Io(io),
            SendHttpResult::Err(err) => {
                let context = self.context.take().unwrap_or_default();
                return SendJmapRequestResult::Err {
                    context,
                    err: err.into(),
                };
            }
        };

        let context = self.context.take().unwrap_or_default();

        if !ok.response.status().is_success() {
            return SendJmapRequestResult::Err {
                context,
                err: SendJmapRequestError::HttpStatus(ok.response.status().as_u16()),
            };
        }

        match serde_json::from_slice::<JmapResponse>(ok.response.body()) {
            Ok(response) => SendJmapRequestResult::Ok {
                context,
                response,
                keep_alive: ok.keep_alive,
            },
            Err(err) => SendJmapRequestResult::Err {
                context,
                err: SendJmapRequestError::ParseResponse(err),
            },
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
