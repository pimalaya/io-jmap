//! Generic I/O-free coroutine for the `Foo/get` method (RFC 8620 §5.1).

use core::marker::PhantomData;

use alloc::{string::String, vec::Vec};

use secrecy::SecretString;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use thiserror::Error;
use url::Url;

use crate::coroutine::*;
use crate::rfc8620::{error::JmapMethodError, send::*};

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

/// Successful terminal output of the [`JmapGet`] coroutine.
#[derive(Clone, Debug)]
pub struct JmapGetOutput<T> {
    pub list: Vec<T>,
    pub not_found: Vec<String>,
    pub state: String,
    pub keep_alive: bool,
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
}

impl<T: DeserializeOwned> JmapCoroutine for JmapGet<T> {
    type Yield = JmapYield;
    type Return = Result<JmapGetOutput<T>, JmapGetError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> JmapCoroutineState<Self::Yield, Self::Return> {
        let JmapSendOutput {
            response,
            keep_alive,
        } = match self.send.resume(arg) {
            JmapCoroutineState::Complete(Ok(out)) => out,
            JmapCoroutineState::Complete(Err(err)) => {
                return JmapCoroutineState::Complete(Err(err.into()));
            }
            JmapCoroutineState::Yielded(y) => return JmapCoroutineState::Yielded(y),
        };

        let Some((name, args, _)) = response.method_responses.into_iter().next() else {
            return JmapCoroutineState::Complete(Err(JmapGetError::MissingResponse));
        };

        if name == "error" {
            let err =
                serde_json::from_value::<JmapMethodError>(args).unwrap_or(JmapMethodError::Unknown);
            return JmapCoroutineState::Complete(Err(err.into()));
        }

        match serde_json::from_value::<GetResponse<T>>(args) {
            Ok(r) => JmapCoroutineState::Complete(Ok(JmapGetOutput {
                list: r.list,
                not_found: r.not_found,
                state: r.state,
                keep_alive,
            })),
            Err(err) => JmapCoroutineState::Complete(Err(JmapGetError::ParseResponse(err))),
        }
    }
}
