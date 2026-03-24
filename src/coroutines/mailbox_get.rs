//! I/O-free coroutine for the standalone `Mailbox/get` method (RFC 8621 §2.5).

use io_stream::io::StreamIo;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    context::JmapContext,
    coroutines::send::{JmapBatch, SendJmapRequest, SendJmapRequestError, SendJmapRequestResult},
    types::{
        error::JmapMethodError,
        mailbox::{Mailbox, MailboxProperty},
        session::capabilities,
    },
};

/// Errors that can occur during the coroutine progression.
#[derive(Debug, Error)]
pub enum GetJmapMailboxesError {
    #[error("Send JMAP request error: {0}")]
    Send(#[from] SendJmapRequestError),
    #[error("Serialize Mailbox/get args error: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    #[error("Parse Mailbox/get response error: {0}")]
    ParseResponse(#[source] serde_json::Error),
    #[error("Missing Mailbox/get response in method_responses")]
    MissingResponse,
    #[error("JMAP Mailbox/get method error: {0}")]
    Method(#[from] JmapMethodError),
}

/// Result returned by the [`GetJmapMailboxes`] coroutine.
#[derive(Debug)]
pub enum GetJmapMailboxesResult {
    Ok {
        context: JmapContext,
        mailboxes: Vec<Mailbox>,
        not_found: Vec<String>,
        new_state: String,
        keep_alive: bool,
    },
    Io(StreamIo),
    Err {
        context: JmapContext,
        err: GetJmapMailboxesError,
    },
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MailboxGetArgs<'a> {
    account_id: &'a str,
    ids: Option<&'a [String]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    properties: Option<&'a [MailboxProperty]>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MailboxGetResponse {
    list: Vec<Mailbox>,
    #[serde(default)]
    not_found: Vec<String>,
    state: String,
}

/// I/O-free coroutine for the JMAP `Mailbox/get` method.
///
/// Fetches mailbox objects by ID. Pass `ids: None` to fetch all mailboxes.
pub struct GetJmapMailboxes {
    send: SendJmapRequest,
}

impl GetJmapMailboxes {
    pub fn new(
        context: JmapContext,
        ids: Option<Vec<String>>,
        properties: Option<Vec<MailboxProperty>>,
    ) -> Result<Self, GetJmapMailboxesError> {
        let account_id = context.account_id.clone().unwrap_or_default();
        let api_url = context
            .api_url()
            .cloned()
            .unwrap_or_else(|| "http://localhost".parse().unwrap());

        let get_args = MailboxGetArgs {
            account_id: &account_id,
            ids: ids.as_deref(),
            properties: properties.as_deref(),
        };

        let mut batch = JmapBatch::new();
        batch.add(
            "Mailbox/get",
            serde_json::to_value(&get_args).map_err(GetJmapMailboxesError::SerializeArgs)?,
        );
        let request =
            batch.into_request(vec![capabilities::CORE.into(), capabilities::MAIL.into()]);

        Ok(Self {
            send: SendJmapRequest::new(context, &api_url, request)?,
        })
    }

    pub fn resume(&mut self, arg: Option<StreamIo>) -> GetJmapMailboxesResult {
        let (context, response, keep_alive) = match self.send.resume(arg) {
            SendJmapRequestResult::Ok { context, response, keep_alive } => {
                (context, response, keep_alive)
            }
            SendJmapRequestResult::Io(io) => return GetJmapMailboxesResult::Io(io),
            SendJmapRequestResult::Err { context, err } => {
                return GetJmapMailboxesResult::Err { context, err: err.into() }
            }
        };

        let Some((name, args, _)) = response.method_responses.into_iter().next() else {
            return GetJmapMailboxesResult::Err {
                context,
                err: GetJmapMailboxesError::MissingResponse,
            };
        };

        if name == "error" {
            let err = serde_json::from_value::<JmapMethodError>(args)
                .unwrap_or(JmapMethodError::Unknown);
            return GetJmapMailboxesResult::Err { context, err: err.into() };
        }

        match serde_json::from_value::<MailboxGetResponse>(args) {
            Ok(r) => GetJmapMailboxesResult::Ok {
                context,
                mailboxes: r.list,
                not_found: r.not_found,
                new_state: r.state,
                keep_alive,
            },
            Err(err) => GetJmapMailboxesResult::Err {
                context,
                err: GetJmapMailboxesError::ParseResponse(err),
            },
        }
    }
}
