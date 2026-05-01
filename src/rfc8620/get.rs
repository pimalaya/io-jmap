//! Generic I/O-free coroutine for the `Foo/get` method (RFC 8620 §5.1).

use alloc::{string::String, vec::Vec};
use core::marker::PhantomData;

use secrecy::SecretString;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use thiserror::Error;
use url::Url;

use crate::rfc8620::{
    error::JmapMethodError,
    send::{JmapBatch, JmapSend, JmapSendError, JmapSendResult},
};

/// Errors that can occur during the coroutine progression.
#[derive(Debug, Error)]
pub enum JmapGetError {
    #[error("Send JMAP request error: {0}")]
    Send(#[from] JmapSendError),
    #[error("Serialize Foo/get args error: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    #[error("Parse Foo/get response error: {0}")]
    ParseResponse(#[source] serde_json::Error),
    #[error("Missing Foo/get response in method_responses")]
    MissingResponse,
    #[error("JMAP Foo/get method error: {0}")]
    Method(#[from] JmapMethodError),
}

/// Result returned by the [`JmapGet`] coroutine.
#[derive(Debug)]
pub enum JmapGetResult<T> {
    /// The coroutine has successfully completed.
    Ok {
        list: Vec<T>,
        not_found: Vec<String>,
        state: String,
        keep_alive: bool,
    },
    /// The coroutine needs more bytes to be read from the socket.
    WantsRead,
    /// The coroutine wants the given bytes to be written to the socket.
    WantsWrite(Vec<u8>),
    /// The coroutine encountered an error.
    Err(JmapGetError),
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GetArgs<'a> {
    account_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    ids: Option<&'a [String]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    properties: Option<&'a [String]>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetResponse<T> {
    list: Vec<T>,
    #[serde(default)]
    not_found: Vec<String>,
    state: String,
}

/// Generic I/O-free coroutine for the JMAP `Foo/get` method (RFC 8620 §5.1).
pub struct JmapGet<T> {
    send: JmapSend,
    _phantom: PhantomData<T>,
}

impl<T: DeserializeOwned> JmapGet<T> {
    /// Creates a new coroutine.
    pub fn new(
        account_id: String,
        http_auth: &SecretString,
        api_url: &Url,
        method: impl Into<String>,
        capabilities: Vec<String>,
        ids: Option<Vec<String>>,
        properties: Option<Vec<String>>,
    ) -> Result<Self, JmapGetError> {
        let args = serde_json::to_value(GetArgs {
            account_id: &account_id,
            ids: ids.as_deref(),
            properties: properties.as_deref(),
        })
        .map_err(JmapGetError::SerializeArgs)?;

        let mut batch = JmapBatch::new();
        batch.add(method, args);
        let request = batch.into_request(capabilities);

        Ok(Self {
            send: JmapSend::new(http_auth, api_url, request)?,
            _phantom: PhantomData,
        })
    }

    /// Creates a coroutine from a pre-built [`JmapSend`].
    pub fn from_send(send: JmapSend) -> Self {
        Self {
            send,
            _phantom: PhantomData,
        }
    }

    /// Advances the coroutine.
    pub fn resume(&mut self, arg: Option<&[u8]>) -> JmapGetResult<T> {
        let (response, keep_alive) = match self.send.resume(arg) {
            JmapSendResult::Ok {
                response,
                keep_alive,
            } => (response, keep_alive),
            JmapSendResult::WantsRead => return JmapGetResult::WantsRead,
            JmapSendResult::WantsWrite(bytes) => return JmapGetResult::WantsWrite(bytes),
            JmapSendResult::Err(err) => return JmapGetResult::Err(err.into()),
        };

        let Some((name, args, _)) = response.method_responses.into_iter().next() else {
            return JmapGetResult::Err(JmapGetError::MissingResponse);
        };

        if name == "error" {
            let err =
                serde_json::from_value::<JmapMethodError>(args).unwrap_or(JmapMethodError::Unknown);
            return JmapGetResult::Err(err.into());
        }

        match serde_json::from_value::<GetResponse<T>>(args) {
            Ok(r) => JmapGetResult::Ok {
                list: r.list,
                not_found: r.not_found,
                state: r.state,
                keep_alive,
            },
            Err(err) => JmapGetResult::Err(JmapGetError::ParseResponse(err)),
        }
    }
}
