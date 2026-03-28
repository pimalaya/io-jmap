//! Generic I/O-free coroutine for the `Foo/set` method (RFC 8620 §5.3).

use std::{collections::HashMap, marker::PhantomData};

use io_stream::io::StreamIo;
use secrecy::SecretString;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use thiserror::Error;

use crate::rfc8620::{
    coroutines::send::{JmapBatch, JmapSend, JmapSendError, JmapSendResult},
    types::{
        error::{JmapMethodError, SetError},
        session::JmapSession,
    },
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
        created: HashMap<String, T>,
        updated: HashMap<String, Option<T>>,
        destroyed: Vec<String>,
        not_created: HashMap<String, SetError>,
        not_updated: HashMap<String, SetError>,
        not_destroyed: HashMap<String, SetError>,
        keep_alive: bool,
    },
    Io {
        io: StreamIo,
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
    create: Option<HashMap<String, C>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    update: Option<HashMap<String, U>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    destroy: Option<Vec<String>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetResponse<T> {
    new_state: String,
    created: Option<HashMap<String, T>>,
    updated: Option<HashMap<String, Option<T>>>,
    destroyed: Option<Vec<String>>,
    not_created: Option<HashMap<String, SetError>>,
    not_updated: Option<HashMap<String, SetError>>,
    not_destroyed: Option<HashMap<String, SetError>>,
}

/// Generic I/O-free coroutine for the JMAP `Foo/set` method (RFC 8620 §5.3).
///
/// Creates, updates, or destroys objects of type `T`. The create map values
/// and update patches are serialized at construction time; pass the appropriate
/// serializable types as `C` (create) and `U` (update).
///
/// `T` must implement [`DeserializeOwned`] to deserialize the objects returned
/// in the `created` and `updated` response fields.
pub struct JmapSet<T> {
    send: JmapSend,
    _phantom: PhantomData<T>,
}

impl<T: DeserializeOwned> JmapSet<T> {
    /// Creates a new coroutine.
    ///
    /// - `method`: JMAP method name, e.g. `"Mailbox/set"`
    /// - `capabilities`: capability URNs to declare
    /// - `if_in_state`: optimistic-locking state; server returns an error if current state differs
    /// - `create`: map of client-assigned ID → object to create
    /// - `update`: map of object ID → patch to apply
    /// - `destroy`: list of object IDs to destroy
    pub fn new<C: Serialize, U: Serialize>(
        session: &JmapSession,
        http_auth: &SecretString,
        method: impl Into<String>,
        capabilities: Vec<String>,
        if_in_state: Option<String>,
        create: Option<HashMap<String, C>>,
        update: Option<HashMap<String, U>>,
        destroy: Option<Vec<String>>,
    ) -> Result<Self, JmapSetError> {
        let account_id = session.primary_account_id();
        let api_url = &session.api_url;

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

        Ok(Self {
            send: JmapSend::new(http_auth, api_url, request)?,
            _phantom: PhantomData,
        })
    }

    /// Creates a coroutine from a pre-built [`JmapSend`].
    ///
    /// Use this when the method requires non-standard request arguments
    /// beyond the standard `create`, `update`, and `destroy` fields.
    pub fn from_send(send: JmapSend) -> Self {
        Self {
            send,
            _phantom: PhantomData,
        }
    }

    /// Makes the coroutine progress.
    pub fn resume(&mut self, arg: Option<StreamIo>) -> JmapSetResult<T> {
        let (response, keep_alive) = match self.send.resume(arg) {
            JmapSendResult::Ok {
                response,
                keep_alive,
            } => (response, keep_alive),
            JmapSendResult::Io { io } => return JmapSetResult::Io { io },
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
