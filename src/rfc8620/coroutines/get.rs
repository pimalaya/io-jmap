//! Generic I/O-free coroutine for the `Foo/get` method (RFC 8620 §5.1).

use std::marker::PhantomData;

use io_stream::io::StreamIo;
use secrecy::SecretString;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use thiserror::Error;

use crate::rfc8620::{
    coroutines::send::{JmapBatch, JmapSend, JmapSendError, JmapSendResult},
    types::{error::JmapMethodError, session::JmapSession},
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
    Ok {
        list: Vec<T>,
        not_found: Vec<String>,
        state: String,
        keep_alive: bool,
    },
    Io {
        io: StreamIo,
    },
    Err {
        err: JmapGetError,
    },
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
///
/// Fetches objects of type `T` by ID. Pass `ids: None` to fetch all objects.
/// `T` must implement [`DeserializeOwned`] to deserialize the server response.
///
/// Use this for any JMAP data type — pass the method name (e.g. `"Email/get"`,
/// `"Mailbox/get"`) and the required capability URNs.
pub struct JmapGet<T> {
    send: JmapSend,
    _phantom: PhantomData<T>,
}

impl<T: DeserializeOwned> JmapGet<T> {
    /// Creates a new coroutine.
    ///
    /// - `method`: JMAP method name, e.g. `"Email/get"`
    /// - `capabilities`: capability URNs to declare (e.g. `vec![CORE.into(), MAIL.into()]`)
    /// - `ids`: object IDs to fetch, or `None` for all objects
    /// - `properties`: object properties to include, or `None` for all
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        method: impl Into<String>,
        capabilities: Vec<String>,
        ids: Option<Vec<String>>,
        properties: Option<Vec<String>>,
    ) -> Result<Self, JmapGetError> {
        let account_id = session.primary_account_id();
        let api_url = &session.api_url;

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
    ///
    /// Use this when the method requires non-standard request arguments
    /// beyond the standard `ids` and `properties` fields.
    pub fn from_send(send: JmapSend) -> Self {
        Self {
            send,
            _phantom: PhantomData,
        }
    }

    /// Makes the coroutine progress.
    pub fn resume(&mut self, arg: Option<StreamIo>) -> JmapGetResult<T> {
        let (response, keep_alive) = match self.send.resume(arg) {
            JmapSendResult::Ok {
                response,
                keep_alive,
            } => (response, keep_alive),
            JmapSendResult::Io { io } => return JmapGetResult::Io { io },
            JmapSendResult::Err { err } => return JmapGetResult::Err { err: err.into() },
        };

        let Some((name, args, _)) = response.method_responses.into_iter().next() else {
            return JmapGetResult::Err {
                err: JmapGetError::MissingResponse,
            };
        };

        if name == "error" {
            let err =
                serde_json::from_value::<JmapMethodError>(args).unwrap_or(JmapMethodError::Unknown);
            return JmapGetResult::Err { err: err.into() };
        }

        match serde_json::from_value::<GetResponse<T>>(args) {
            Ok(r) => JmapGetResult::Ok {
                list: r.list,
                not_found: r.not_found,
                state: r.state,
                keep_alive,
            },
            Err(err) => JmapGetResult::Err {
                err: JmapGetError::ParseResponse(err),
            },
        }
    }
}
