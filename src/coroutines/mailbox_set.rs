//! I/O-free coroutine for the `Mailbox/set` method (RFC 8621 §2.6).

use std::collections::HashMap;

use io_stream::io::StreamIo;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    context::JmapContext,
    coroutines::send::{JmapBatch, SendJmapRequest, SendJmapRequestError, SendJmapRequestResult},
    types::{
        error::JmapMethodError,
        mailbox::{Mailbox, MailboxCreate, MailboxUpdate},
        session::capabilities,
    },
};

/// Errors that can occur during the coroutine progression.
#[derive(Debug, Error)]
pub enum SetJmapMailboxesError {
    #[error("Send JMAP request error: {0}")]
    Send(#[from] SendJmapRequestError),
    #[error("Serialize Mailbox/set args error: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    #[error("Parse Mailbox/set response error: {0}")]
    ParseResponse(#[source] serde_json::Error),
    #[error("Missing Mailbox/set response in method_responses")]
    MissingResponse,
    #[error("JMAP Mailbox/set method error: {0}")]
    Method(#[from] JmapMethodError),
}

/// Per-object error from a `Mailbox/set` response.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetError {
    #[serde(rename = "type")]
    pub error_type: String,
    pub description: Option<String>,
    /// For `invalidProperties` errors: the list of offending property names.
    #[serde(default)]
    pub properties: Vec<String>,
}

/// Result returned by the [`SetJmapMailboxes`] coroutine.
#[derive(Debug)]
pub enum SetJmapMailboxesResult {
    Ok {
        context: JmapContext,
        new_state: String,
        created: HashMap<String, Mailbox>,
        updated: HashMap<String, Option<Mailbox>>,
        destroyed: Vec<String>,
        not_created: HashMap<String, SetError>,
        not_updated: HashMap<String, SetError>,
        not_destroyed: HashMap<String, SetError>,
        keep_alive: bool,
    },
    Io(StreamIo),
    Err {
        context: JmapContext,
        err: SetJmapMailboxesError,
    },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MailboxSetResponse {
    new_state: String,
    created: Option<HashMap<String, Mailbox>>,
    updated: Option<HashMap<String, Option<Mailbox>>>,
    destroyed: Option<Vec<String>>,
    not_created: Option<HashMap<String, SetError>>,
    not_updated: Option<HashMap<String, SetError>>,
    not_destroyed: Option<HashMap<String, SetError>>,
}

/// Arguments for a `Mailbox/set` request.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MailboxSetArgs {
    /// Objects to create (client ID → partial mailbox object).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create: Option<HashMap<String, MailboxCreate>>,

    /// Objects to update (mailbox ID → patch object).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update: Option<HashMap<String, MailboxUpdate>>,

    /// IDs of objects to destroy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destroy: Option<Vec<String>>,

    /// Whether to destroy empty messages when destroying a mailbox.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_destroy_remove_emails: Option<bool>,
}

#[derive(Serialize)]
struct MailboxSetRequest {
    #[serde(rename = "accountId")]
    account_id: String,
    #[serde(flatten)]
    args: MailboxSetArgs,
}

/// I/O-free coroutine for the JMAP `Mailbox/set` method.
///
/// Creates, updates, or destroys mailbox objects.
pub struct SetJmapMailboxes {
    send: SendJmapRequest,
}

impl SetJmapMailboxes {
    /// Creates a new coroutine.
    pub fn new(context: JmapContext, args: MailboxSetArgs) -> Result<Self, SetJmapMailboxesError> {
        let account_id = context.account_id.clone().unwrap_or_default();
        let api_url = context
            .api_url()
            .cloned()
            .unwrap_or_else(|| "http://localhost".parse().unwrap());

        let json_args = serde_json::to_value(MailboxSetRequest { account_id, args })
            .map_err(SetJmapMailboxesError::SerializeArgs)?;

        let mut batch = JmapBatch::new();
        batch.add("Mailbox/set", json_args);
        let request =
            batch.into_request(vec![capabilities::CORE.into(), capabilities::MAIL.into()]);

        Ok(Self {
            send: SendJmapRequest::new(context, &api_url, request)?,
        })
    }

    /// Makes the coroutine progress.
    pub fn resume(&mut self, arg: Option<StreamIo>) -> SetJmapMailboxesResult {
        let (context, response, keep_alive) = match self.send.resume(arg) {
            SendJmapRequestResult::Ok {
                context,
                response,
                keep_alive,
            } => (context, response, keep_alive),
            SendJmapRequestResult::Io(io) => return SetJmapMailboxesResult::Io(io),
            SendJmapRequestResult::Err { context, err } => {
                return SetJmapMailboxesResult::Err {
                    context,
                    err: err.into(),
                }
            }
        };

        let Some((name, args, _)) = response.method_responses.into_iter().next() else {
            return SetJmapMailboxesResult::Err {
                context,
                err: SetJmapMailboxesError::MissingResponse,
            };
        };

        if name == "error" {
            let err =
                serde_json::from_value::<JmapMethodError>(args).unwrap_or(JmapMethodError::Unknown);
            return SetJmapMailboxesResult::Err {
                context,
                err: err.into(),
            };
        }

        match serde_json::from_value::<MailboxSetResponse>(args) {
            Ok(r) => SetJmapMailboxesResult::Ok {
                context,
                new_state: r.new_state,
                created: r.created.unwrap_or_default(),
                updated: r.updated.unwrap_or_default(),
                destroyed: r.destroyed.unwrap_or_default(),
                not_created: r.not_created.unwrap_or_default(),
                not_updated: r.not_updated.unwrap_or_default(),
                not_destroyed: r.not_destroyed.unwrap_or_default(),
                keep_alive,
            },
            Err(err) => SetJmapMailboxesResult::Err {
                context,
                err: SetJmapMailboxesError::ParseResponse(err),
            },
        }
    }
}
