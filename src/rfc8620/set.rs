//! Generic I/O-free coroutine for the `Foo/set` method (RFC 8620 §5.3).

use alloc::{collections::BTreeMap, string::String, vec::Vec};
use core::marker::PhantomData;

use io_socket::io::{SocketInput, SocketOutput};
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
pub enum JmapSetError {
    #[error("Send JMAP request error: {0}")]
    Send(#[from] JmapSendError),
    #[error("Serialize Foo/set args error: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    #[error("Parse Foo/set response error: {0}")]
    ParseResponse(#[source] serde_json::Error),
    #[error("Missing Foo/set response in method_responses")]
    MissingResponse,
    #[error("JMAP Foo/set method error: {0}")]
    Method(#[from] JmapMethodError),
}

/// Result returned by the [`JmapSet`] coroutine.
#[derive(Debug)]
pub enum JmapSetResult<T> {
    Ok {
        new_state: String,
        created: BTreeMap<String, T>,
        updated: BTreeMap<String, Option<T>>,
        destroyed: Vec<String>,
        not_created: BTreeMap<String, serde_json::Value>,
        not_updated: BTreeMap<String, serde_json::Value>,
        not_destroyed: BTreeMap<String, serde_json::Value>,
        keep_alive: bool,
    },
    Io {
        input: SocketInput,
    },
    Err {
        err: JmapSetError,
    },
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SetArgs<C: Serialize, U: Serialize> {
    account_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    if_in_state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    create: Option<BTreeMap<String, C>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    update: Option<BTreeMap<String, U>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    destroy: Option<Vec<String>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetResponse<T> {
    new_state: String,
    created: Option<BTreeMap<String, T>>,
    updated: Option<BTreeMap<String, Option<T>>>,
    destroyed: Option<Vec<String>>,
    not_created: Option<BTreeMap<String, serde_json::Value>>,
    not_updated: Option<BTreeMap<String, serde_json::Value>>,
    not_destroyed: Option<BTreeMap<String, serde_json::Value>>,
}

/// Generic I/O-free coroutine for the JMAP `Foo/set` method (RFC 8620 §5.3).
pub struct JmapSet<T> {
    send: JmapSend,
    _phantom: PhantomData<T>,
}

impl<T: DeserializeOwned> JmapSet<T> {
    /// Creates a new coroutine.
    pub fn new<C: Serialize, U: Serialize>(
        account_id: String,
        http_auth: &SecretString,
        api_url: &Url,
        method: impl Into<String>,
        capabilities: Vec<String>,
        if_in_state: Option<String>,
        create: Option<BTreeMap<String, C>>,
        update: Option<BTreeMap<String, U>>,
        destroy: Option<Vec<String>>,
    ) -> Result<Self, JmapSetError> {
        let args = serde_json::to_value(SetArgs {
            account_id,
            if_in_state,
            create,
            update,
            destroy,
        })
        .map_err(JmapSetError::SerializeArgs)?;

        let mut batch = JmapBatch::new();
        batch.add(method, args);

        let request = batch.into_request(capabilities);
        let send = JmapSend::new(http_auth, api_url, request)?;

        Ok(Self {
            send,
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

    /// Makes the coroutine progress.
    pub fn resume(&mut self, arg: Option<SocketOutput>) -> JmapSetResult<T> {
        let (response, keep_alive) = match self.send.resume(arg) {
            JmapSendResult::Ok {
                response,
                keep_alive,
            } => (response, keep_alive),
            JmapSendResult::Io { input } => return JmapSetResult::Io { input },
            JmapSendResult::Err { err } => return JmapSetResult::Err { err: err.into() },
        };

        let Some((name, args, _)) = response.method_responses.into_iter().next() else {
            return JmapSetResult::Err {
                err: JmapSetError::MissingResponse,
            };
        };

        if name == "error" {
            let err =
                serde_json::from_value::<JmapMethodError>(args).unwrap_or(JmapMethodError::Unknown);
            return JmapSetResult::Err { err: err.into() };
        }

        match serde_json::from_value::<SetResponse<T>>(args) {
            Ok(r) => JmapSetResult::Ok {
                new_state: r.new_state,
                created: r.created.unwrap_or_default(),
                updated: r.updated.unwrap_or_default(),
                destroyed: r.destroyed.unwrap_or_default(),
                not_created: r.not_created.unwrap_or_default(),
                not_updated: r.not_updated.unwrap_or_default(),
                not_destroyed: r.not_destroyed.unwrap_or_default(),
                keep_alive,
            },
            Err(err) => JmapSetResult::Err {
                err: JmapSetError::ParseResponse(err),
            },
        }
    }
}
