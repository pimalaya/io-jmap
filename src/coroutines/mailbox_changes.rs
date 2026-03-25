//! I/O-free coroutine for `Mailbox/changes` (RFC 8621 §2.7).

use io_stream::io::StreamIo;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    context::JmapContext,
    coroutines::send::{JmapBatch, SendJmapRequest, SendJmapRequestError, SendJmapRequestResult},
    types::{error::JmapMethodError, session::capabilities},
};

/// Errors that can occur during the coroutine progression.
#[derive(Debug, Error)]
pub enum GetJmapMailboxChangesError {
    #[error("Send JMAP request error: {0}")]
    Send(#[from] SendJmapRequestError),
    #[error("Serialize Mailbox/changes args error: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    #[error("Parse Mailbox/changes response error: {0}")]
    ParseResponse(#[source] serde_json::Error),
    #[error("Missing Mailbox/changes response in method_responses")]
    MissingResponse,
    #[error("JMAP Mailbox/changes method error: {0}")]
    Method(#[from] JmapMethodError),
}

/// Result returned by the [`GetJmapMailboxChanges`] coroutine.
#[derive(Debug)]
pub enum GetJmapMailboxChangesResult {
    Ok {
        context: JmapContext,
        new_state: String,
        has_more_changes: bool,
        created: Vec<String>,
        updated: Vec<String>,
        destroyed: Vec<String>,
        keep_alive: bool,
    },
    Io(StreamIo),
    Err {
        context: JmapContext,
        err: GetJmapMailboxChangesError,
    },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MailboxChangesResponse {
    new_state: String,
    has_more_changes: bool,
    created: Vec<String>,
    updated: Vec<String>,
    destroyed: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MailboxChangesArgs {
    account_id: String,
    since_state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_changes: Option<u64>,
}

/// I/O-free coroutine for the JMAP `Mailbox/changes` method.
///
/// Returns the changes to mailboxes since the given `since_state`.
pub struct GetJmapMailboxChanges {
    send: SendJmapRequest,
}

impl GetJmapMailboxChanges {
    /// Creates a new coroutine.
    pub fn new(
        context: JmapContext,
        since_state: impl Into<String>,
        max_changes: Option<u64>,
    ) -> Result<Self, GetJmapMailboxChangesError> {
        let account_id = context.account_id.clone().unwrap_or_default();
        let api_url = context
            .api_url()
            .cloned()
            .unwrap_or_else(|| "http://localhost".parse().unwrap());

        let args = serde_json::to_value(MailboxChangesArgs {
            account_id,
            since_state: since_state.into(),
            max_changes,
        })
        .map_err(GetJmapMailboxChangesError::SerializeArgs)?;

        let mut batch = JmapBatch::new();
        batch.add("Mailbox/changes", args);
        let request = batch.into_request(vec![
            capabilities::CORE.into(),
            capabilities::MAIL.into(),
        ]);

        Ok(Self {
            send: SendJmapRequest::new(context, &api_url, request)?,
        })
    }

    /// Makes the coroutine progress.
    pub fn resume(&mut self, arg: Option<StreamIo>) -> GetJmapMailboxChangesResult {
        let (context, response, keep_alive) = match self.send.resume(arg) {
            SendJmapRequestResult::Ok { context, response, keep_alive } => {
                (context, response, keep_alive)
            }
            SendJmapRequestResult::Io(io) => return GetJmapMailboxChangesResult::Io(io),
            SendJmapRequestResult::Err { context, err } => {
                return GetJmapMailboxChangesResult::Err {
                    context,
                    err: err.into(),
                }
            }
        };

        let Some((name, args, _)) = response.method_responses.into_iter().next() else {
            return GetJmapMailboxChangesResult::Err {
                context,
                err: GetJmapMailboxChangesError::MissingResponse,
            };
        };

        if name == "error" {
            let err = serde_json::from_value::<JmapMethodError>(args)
                .unwrap_or(JmapMethodError::Unknown);
            return GetJmapMailboxChangesResult::Err {
                context,
                err: err.into(),
            };
        }

        match serde_json::from_value::<MailboxChangesResponse>(args) {
            Ok(r) => GetJmapMailboxChangesResult::Ok {
                context,
                new_state: r.new_state,
                has_more_changes: r.has_more_changes,
                created: r.created,
                updated: r.updated,
                destroyed: r.destroyed,
                keep_alive,
            },
            Err(err) => GetJmapMailboxChangesResult::Err {
                context,
                err: GetJmapMailboxChangesError::ParseResponse(err),
            },
        }
    }
}
