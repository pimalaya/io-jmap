//! Generic I/O-free coroutine for the `Foo/changes` method (RFC 8620 §5.2).

use io_stream::io::StreamIo;
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::rfc8620::{
    coroutines::send::{JmapBatch, JmapSend, JmapSendError, JmapSendResult},
    types::{error::JmapMethodError, session::JmapSession},
};

/// Errors that can occur during the coroutine progression.
#[derive(Debug, Error)]
pub enum JmapChangesError {
    #[error("Send JMAP request error: {0}")]
    Send(#[from] JmapSendError),
    #[error("Serialize Foo/changes args error: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    #[error("Parse Foo/changes response error: {0}")]
    ParseResponse(#[source] serde_json::Error),
    #[error("Missing Foo/changes response in method_responses")]
    MissingResponse,
    #[error("JMAP Foo/changes method error: {0}")]
    Method(#[from] JmapMethodError),
}

/// Result returned by the [`JmapChanges`] coroutine.
#[derive(Debug)]
pub enum JmapChangesResult {
    Ok {
        new_state: String,
        has_more_changes: bool,
        created: Vec<String>,
        updated: Vec<String>,
        destroyed: Vec<String>,
        keep_alive: bool,
    },
    Io {
        io: StreamIo,
    },
    Err {
        err: JmapChangesError,
    },
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ChangesArgs<'a> {
    account_id: &'a str,
    since_state: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_changes: Option<u64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChangesResponse {
    new_state: String,
    has_more_changes: bool,
    created: Vec<String>,
    updated: Vec<String>,
    destroyed: Vec<String>,
}

/// Generic I/O-free coroutine for the JMAP `Foo/changes` method (RFC 8620 §5.2).
///
/// Returns the set of IDs that have been created, updated, or destroyed since
/// `since_state`. Works for any JMAP data type — pass the method name
/// (e.g. `"Email/changes"`, `"Mailbox/changes"`) and the required capabilities.
pub struct JmapChanges {
    send: JmapSend,
}

impl JmapChanges {
    /// Creates a new coroutine.
    ///
    /// - `method`: JMAP method name, e.g. `"Email/changes"`
    /// - `capabilities`: capability URNs to declare
    /// - `since_state`: the state string from a previous response
    /// - `max_changes`: limit on the number of changes returned; `None` for server default
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        method: impl Into<String>,
        capabilities: Vec<String>,
        since_state: impl Into<String>,
        max_changes: Option<u64>,
    ) -> Result<Self, JmapChangesError> {
        let account_id = session.primary_account_id();
        let since_state = since_state.into();
        let api_url = &session.api_url;

        let args = serde_json::to_value(ChangesArgs {
            account_id: &account_id,
            since_state: &since_state,
            max_changes,
        })
        .map_err(JmapChangesError::SerializeArgs)?;

        let mut batch = JmapBatch::new();
        batch.add(method, args);
        let request = batch.into_request(capabilities);

        Ok(Self {
            send: JmapSend::new(http_auth, api_url, request)?,
        })
    }

    /// Creates a coroutine from a pre-built [`JmapSend`].
    pub fn from_send(send: JmapSend) -> Self {
        Self { send }
    }

    /// Makes the coroutine progress.
    pub fn resume(&mut self, arg: Option<StreamIo>) -> JmapChangesResult {
        let (response, keep_alive) = match self.send.resume(arg) {
            JmapSendResult::Ok {
                response,
                keep_alive,
            } => (response, keep_alive),
            JmapSendResult::Io { io } => return JmapChangesResult::Io { io },
            JmapSendResult::Err { err } => return JmapChangesResult::Err { err: err.into() },
        };

        let Some((name, args, _)) = response.method_responses.into_iter().next() else {
            return JmapChangesResult::Err {
                err: JmapChangesError::MissingResponse,
            };
        };

        if name == "error" {
            let err =
                serde_json::from_value::<JmapMethodError>(args).unwrap_or(JmapMethodError::Unknown);
            return JmapChangesResult::Err { err: err.into() };
        }

        match serde_json::from_value::<ChangesResponse>(args) {
            Ok(r) => JmapChangesResult::Ok {
                new_state: r.new_state,
                has_more_changes: r.has_more_changes,
                created: r.created,
                updated: r.updated,
                destroyed: r.destroyed,
                keep_alive,
            },
            Err(err) => JmapChangesResult::Err {
                err: JmapChangesError::ParseResponse(err),
            },
        }
    }
}
