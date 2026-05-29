//! I/O-free coroutine for the `Email/set` method (RFC 8621 §4.7).

use alloc::{collections::BTreeMap, string::String, vec, vec::Vec};

use secrecy::SecretString;
use serde::Serialize;
use thiserror::Error;

use crate::coroutine::*;
use crate::{
    rfc8620::{send::*, session::JmapSession, set::*},
    rfc8621::{
        capabilities,
        email::{Email, EmailPatch, EmailPatchOp, EmailSetError},
    },
};

/// Errors that can occur during the coroutine progression.
#[derive(Debug, Error)]
pub enum JmapEmailSetError {
    #[error("Send JMAP request error: {0}")]
    Send(#[from] JmapSendError),
    #[error("Serialize Email/set args error: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    #[error("JMAP Email/set error: {0}")]
    Set(#[from] JmapSetError),
}

/// Successful terminal output of [`JmapEmailSet`].
#[derive(Clone, Debug)]
pub struct JmapEmailSetOutput {
    pub new_state: String,
    pub created: BTreeMap<String, Email>,
    pub updated: BTreeMap<String, Option<Email>>,
    pub destroyed: Vec<String>,
    pub not_created: BTreeMap<String, EmailSetError>,
    pub not_updated: BTreeMap<String, EmailSetError>,
    pub not_destroyed: BTreeMap<String, EmailSetError>,
    pub keep_alive: bool,
}

/// Arguments for an `Email/set` request.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapEmailSetArgs {
    /// Objects to create (client ID → partial email object).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create: Option<BTreeMap<String, Email>>,

    /// Objects to update (email ID → patch).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update: Option<BTreeMap<String, EmailPatch>>,

    /// IDs to destroy (delete).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destroy: Option<Vec<String>>,
}

impl JmapEmailSetArgs {
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

    pub fn set_keyword(&mut self, id: impl Into<String>, keyword: impl Into<String>) -> &mut Self {
        self.patch(id)
            .0
            .push(EmailPatchOp::SetKeyword(keyword.into()));
        self
    }

    pub fn unset_keyword(
        &mut self,
        id: impl Into<String>,
        keyword: impl Into<String>,
    ) -> &mut Self {
        self.patch(id)
            .0
            .push(EmailPatchOp::UnsetKeyword(keyword.into()));
        self
    }

    pub fn replace_keywords(
        &mut self,
        id: impl Into<String>,
        keywords: BTreeMap<String, bool>,
    ) -> &mut Self {
        self.patch(id)
            .0
            .push(EmailPatchOp::ReplaceKeywords(keywords));
        self
    }

    pub fn add_to_mailbox(
        &mut self,
        id: impl Into<String>,
        mailbox_id: impl Into<String>,
    ) -> &mut Self {
        self.patch(id)
            .0
            .push(EmailPatchOp::AddToMailbox(mailbox_id.into()));
        self
    }

    pub fn remove_from_mailbox(
        &mut self,
        id: impl Into<String>,
        mailbox_id: impl Into<String>,
    ) -> &mut Self {
        self.patch(id)
            .0
            .push(EmailPatchOp::RemoveFromMailbox(mailbox_id.into()));
        self
    }

    pub fn replace_mailbox_ids(
        &mut self,
        id: impl Into<String>,
        ids: BTreeMap<String, bool>,
    ) -> &mut Self {
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
    args: JmapEmailSetArgs,
}

/// I/O-free coroutine for the JMAP `Email/set` method.
///
/// Creates, updates (e.g. sets keywords/flags), or destroys email objects.
pub struct JmapEmailSet {
    set: JmapSet<Email>,
}

impl JmapEmailSet {
    /// Creates a new coroutine.
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        args: JmapEmailSetArgs,
    ) -> Result<Self, JmapEmailSetError> {
        let account_id = session
            .primary_accounts
            .get(capabilities::MAIL)
            .cloned()
            .unwrap_or_default();
        let api_url = &session.api_url;

        let json_args = serde_json::to_value(EmailSetRequest { account_id, args })
            .map_err(JmapEmailSetError::SerializeArgs)?;

        let mut batch = JmapBatch::new();
        batch.add("Email/set", json_args);
        let request =
            batch.into_request(vec![capabilities::CORE.into(), capabilities::MAIL.into()]);

        let send = JmapSend::new(http_auth, api_url, request)?;
        Ok(Self {
            set: JmapSet::from_send(send),
        })
    }
}

impl JmapCoroutine for JmapEmailSet {
    type Yield = JmapYield;
    type Return = Result<JmapEmailSetOutput, JmapEmailSetError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> JmapCoroutineState<Self::Yield, Self::Return> {
        match self.set.resume(arg) {
            JmapCoroutineState::Complete(Ok(JmapSetOutput {
                new_state,
                created,
                updated,
                destroyed,
                not_created,
                not_updated,
                not_destroyed,
                keep_alive,
            })) => {
                let parse = |map: BTreeMap<String, serde_json::Value>| {
                    map.into_iter()
                        .map(|(k, v)| {
                            let e = serde_json::from_value(v).unwrap_or(EmailSetError::Unknown);
                            (k, e)
                        })
                        .collect()
                };
                JmapCoroutineState::Complete(Ok(JmapEmailSetOutput {
                    new_state,
                    created,
                    updated,
                    destroyed,
                    not_created: parse(not_created),
                    not_updated: parse(not_updated),
                    not_destroyed: parse(not_destroyed),
                    keep_alive,
                }))
            }
            JmapCoroutineState::Complete(Err(err)) => JmapCoroutineState::Complete(Err(err.into())),
            JmapCoroutineState::Yielded(y) => JmapCoroutineState::Yielded(y),
        }
    }
}
