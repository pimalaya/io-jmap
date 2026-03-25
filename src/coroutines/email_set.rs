//! I/O-free coroutine for the `Email/set` method (RFC 8621 §4.7).

use std::collections::HashMap;

use io_stream::io::StreamIo;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    context::JmapContext,
    coroutines::send::{JmapBatch, SendJmapRequest, SendJmapRequestError, SendJmapRequestResult},
    types::{
        email::{Email, EmailPatch, EmailPatchOp},
        error::JmapMethodError,
        session::capabilities,
    },
};

/// Errors that can occur during the coroutine progression.
#[derive(Debug, Error)]
pub enum SetJmapEmailsError {
    #[error("Send JMAP request error: {0}")]
    Send(#[from] SendJmapRequestError),
    #[error("Serialize Email/set args error: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    #[error("Parse Email/set response error: {0}")]
    ParseResponse(#[source] serde_json::Error),
    #[error("Missing Email/set response in method_responses")]
    MissingResponse,
    #[error("JMAP Email/set method error: {0}")]
    Method(#[from] JmapMethodError),
}

/// Per-object set error.
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

/// Result returned by the [`SetJmapEmails`] coroutine.
#[derive(Debug)]
pub enum SetJmapEmailsResult {
    Ok {
        context: JmapContext,
        new_state: String,
        created: HashMap<String, Email>,
        updated: HashMap<String, Option<Email>>,
        destroyed: Vec<String>,
        not_created: HashMap<String, SetError>,
        not_updated: HashMap<String, SetError>,
        not_destroyed: HashMap<String, SetError>,
        keep_alive: bool,
    },
    Io(StreamIo),
    Err {
        context: JmapContext,
        err: SetJmapEmailsError,
    },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EmailSetResponse {
    new_state: String,
    #[serde(default)]
    created: Option<HashMap<String, Email>>,
    #[serde(default)]
    updated: Option<HashMap<String, Option<Email>>>,
    #[serde(default)]
    destroyed: Option<Vec<String>>,
    #[serde(default)]
    not_created: Option<HashMap<String, SetError>>,
    #[serde(default)]
    not_updated: Option<HashMap<String, SetError>>,
    #[serde(default)]
    not_destroyed: Option<HashMap<String, SetError>>,
}

/// Arguments for an `Email/set` request.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmailSetArgs {
    /// Objects to create (client ID → partial email object).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create: Option<HashMap<String, Email>>,

    /// Objects to update (email ID → patch).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update: Option<HashMap<String, EmailPatch>>,

    /// IDs to destroy (delete).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destroy: Option<Vec<String>>,
}

impl EmailSetArgs {
    /// Queue an email for creation under the given client-chosen ID.
    pub fn create(&mut self, client_id: impl Into<String>, email: Email) -> &mut Self {
        self.create
            .get_or_insert_with(Default::default)
            .insert(client_id.into(), email);
        self
    }

    /// Queue an email ID for destruction.
    pub fn destroy(&mut self, id: impl Into<String>) -> &mut Self {
        self.destroy
            .get_or_insert_with(Default::default)
            .push(id.into());
        self
    }

    // --- patch helpers (mirror EmailPatch, but routed by email ID) ---

    pub fn set_keyword(&mut self, id: impl Into<String>, keyword: impl Into<String>) -> &mut Self {
        self.patch(id).0.push(EmailPatchOp::SetKeyword(keyword.into()));
        self
    }

    pub fn unset_keyword(&mut self, id: impl Into<String>, keyword: impl Into<String>) -> &mut Self {
        self.patch(id).0.push(EmailPatchOp::UnsetKeyword(keyword.into()));
        self
    }

    pub fn replace_keywords(&mut self, id: impl Into<String>, keywords: HashMap<String, bool>) -> &mut Self {
        self.patch(id).0.push(EmailPatchOp::ReplaceKeywords(keywords));
        self
    }

    pub fn add_to_mailbox(&mut self, id: impl Into<String>, mailbox_id: impl Into<String>) -> &mut Self {
        self.patch(id).0.push(EmailPatchOp::AddToMailbox(mailbox_id.into()));
        self
    }

    pub fn remove_from_mailbox(&mut self, id: impl Into<String>, mailbox_id: impl Into<String>) -> &mut Self {
        self.patch(id).0.push(EmailPatchOp::RemoveFromMailbox(mailbox_id.into()));
        self
    }

    pub fn replace_mailbox_ids(&mut self, id: impl Into<String>, ids: HashMap<String, bool>) -> &mut Self {
        self.patch(id).0.push(EmailPatchOp::ReplaceMailboxIds(ids));
        self
    }

    fn patch(&mut self, id: impl Into<String>) -> &mut EmailPatch {
        self.update
            .get_or_insert_with(Default::default)
            .entry(id.into())
            .or_default()
    }
}

#[derive(Serialize)]
struct EmailSetRequest {
    #[serde(rename = "accountId")]
    account_id: String,
    #[serde(flatten)]
    args: EmailSetArgs,
}

/// I/O-free coroutine for the JMAP `Email/set` method.
///
/// Creates, updates (e.g. sets keywords/flags), or destroys email objects.
pub struct SetJmapEmails {
    send: SendJmapRequest,
}

impl SetJmapEmails {
    /// Creates a new coroutine.
    pub fn new(context: JmapContext, args: EmailSetArgs) -> Result<Self, SetJmapEmailsError> {
        let account_id = context.account_id.clone().unwrap_or_default();
        let api_url = context
            .api_url()
            .cloned()
            .unwrap_or_else(|| "http://localhost".parse().unwrap());

        let json_args = serde_json::to_value(EmailSetRequest { account_id, args })
            .map_err(SetJmapEmailsError::SerializeArgs)?;

        let mut batch = JmapBatch::new();
        batch.add("Email/set", json_args);
        let request =
            batch.into_request(vec![capabilities::CORE.into(), capabilities::MAIL.into()]);

        Ok(Self {
            send: SendJmapRequest::new(context, &api_url, request)?,
        })
    }

    /// Makes the coroutine progress.
    pub fn resume(&mut self, arg: Option<StreamIo>) -> SetJmapEmailsResult {
        let (context, response, keep_alive) = match self.send.resume(arg) {
            SendJmapRequestResult::Ok {
                context,
                response,
                keep_alive,
            } => (context, response, keep_alive),
            SendJmapRequestResult::Io(io) => return SetJmapEmailsResult::Io(io),
            SendJmapRequestResult::Err { context, err } => {
                return SetJmapEmailsResult::Err {
                    context,
                    err: err.into(),
                }
            }
        };

        let Some((name, args, _)) = response.method_responses.into_iter().next() else {
            return SetJmapEmailsResult::Err {
                context,
                err: SetJmapEmailsError::MissingResponse,
            };
        };

        if name == "error" {
            let err =
                serde_json::from_value::<JmapMethodError>(args).unwrap_or(JmapMethodError::Unknown);
            return SetJmapEmailsResult::Err {
                context,
                err: err.into(),
            };
        }

        match serde_json::from_value::<EmailSetResponse>(args) {
            Ok(r) => SetJmapEmailsResult::Ok {
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
            Err(err) => SetJmapEmailsResult::Err {
                context,
                err: SetJmapEmailsError::ParseResponse(err),
            },
        }
    }
}
