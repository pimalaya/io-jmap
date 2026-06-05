//! JMAP `Email/set` coroutine (RFC 8621 §4.7): wraps the generic [`JmapSet`]
//! with [`JmapEmailSetArgs`] (create/update/destroy) and decodes per-object
//! [`JmapEmailSetItemError`] payloads.
//!
//! # Example
//!
//! ```rust,no_run
//! use io_jmap::{
//!     rfc8620::JmapSession,
//!     rfc8621::email::set::{JmapEmailSet, JmapEmailSetArgs},
//! };
//! use secrecy::SecretString;
//!
//! # fn demo(session: &JmapSession) {
//! let auth = SecretString::from("Bearer xyz");
//! let mut args = JmapEmailSetArgs::default();
//! args.destroy("e1");
//! let coroutine = JmapEmailSet::new(session, &auth, args).unwrap();
//! # let _ = coroutine;
//! # }
//! ```

use core::fmt;

use alloc::{collections::BTreeMap, string::String, vec, vec::Vec};

use log::trace;
use secrecy::SecretString;
use serde::Serialize;
use thiserror::Error;

use crate::{
    coroutine::*,
    jmap_try,
    rfc8620::{CORE_CAPABILITY, JmapBatch, JmapSession, send::*, set::*},
    rfc8621::{
        MAIL_CAPABILITY,
        email::{JmapEmail, JmapEmailPatch, JmapEmailPatchOp, JmapEmailSetItemError},
    },
};

/// Failure causes during a JMAP `Email/set` flow.
#[derive(Debug, Error)]
pub enum JmapEmailSetError {
    #[error("JMAP Email/set failed: {0}")]
    Send(#[from] JmapSendError),
    #[error("JMAP Email/set failed: serialize args: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    #[error("JMAP Email/set failed: {0}")]
    Set(#[from] JmapSetError),
}

/// Arguments for an `Email/set` request.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapEmailSetArgs {
    /// Objects to create (client ID → partial email object).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create: Option<BTreeMap<String, JmapEmail>>,

    /// Objects to update (email ID → patch).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update: Option<BTreeMap<String, JmapEmailPatch>>,

    /// IDs to destroy (delete).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destroy: Option<Vec<String>>,
}

impl JmapEmailSetArgs {
    /// Queue an email for creation under the given client-chosen ID.
    pub fn create(&mut self, client_id: impl Into<String>, email: JmapEmail) -> &mut Self {
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
            .push(JmapEmailPatchOp::SetKeyword(keyword.into()));
        self
    }

    pub fn unset_keyword(
        &mut self,
        id: impl Into<String>,
        keyword: impl Into<String>,
    ) -> &mut Self {
        self.patch(id)
            .0
            .push(JmapEmailPatchOp::UnsetKeyword(keyword.into()));
        self
    }

    pub fn replace_keywords(
        &mut self,
        id: impl Into<String>,
        keywords: BTreeMap<String, bool>,
    ) -> &mut Self {
        self.patch(id)
            .0
            .push(JmapEmailPatchOp::ReplaceKeywords(keywords));
        self
    }

    pub fn add_to_mailbox(
        &mut self,
        id: impl Into<String>,
        mailbox_id: impl Into<String>,
    ) -> &mut Self {
        self.patch(id)
            .0
            .push(JmapEmailPatchOp::AddToMailbox(mailbox_id.into()));
        self
    }

    pub fn remove_from_mailbox(
        &mut self,
        id: impl Into<String>,
        mailbox_id: impl Into<String>,
    ) -> &mut Self {
        self.patch(id)
            .0
            .push(JmapEmailPatchOp::RemoveFromMailbox(mailbox_id.into()));
        self
    }

    pub fn replace_mailbox_ids(
        &mut self,
        id: impl Into<String>,
        ids: BTreeMap<String, bool>,
    ) -> &mut Self {
        self.patch(id)
            .0
            .push(JmapEmailPatchOp::ReplaceMailboxIds(ids));
        self
    }

    fn patch(&mut self, id: impl Into<String>) -> &mut JmapEmailPatch {
        self.update
            .get_or_insert_with(Default::default)
            .entry(id.into())
            .or_default()
    }
}

/// Successful terminal output of [`JmapEmailSet`].
#[derive(Clone, Debug)]
pub struct JmapEmailSetOutput {
    pub new_state: String,
    pub created: BTreeMap<String, JmapEmail>,
    pub updated: BTreeMap<String, Option<JmapEmail>>,
    pub destroyed: Vec<String>,
    pub not_created: BTreeMap<String, JmapEmailSetItemError>,
    pub not_updated: BTreeMap<String, JmapEmailSetItemError>,
    pub not_destroyed: BTreeMap<String, JmapEmailSetItemError>,
    pub keep_alive: bool,
}

/// I/O-free coroutine for the JMAP `Email/set` method.
pub struct JmapEmailSet {
    state: State,
}

impl JmapEmailSet {
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        args: JmapEmailSetArgs,
    ) -> Result<Self, JmapEmailSetError> {
        let account_id = session
            .primary_accounts
            .get(MAIL_CAPABILITY)
            .cloned()
            .unwrap_or_default();
        let api_url = &session.api_url;

        let json_args = serde_json::to_value(EmailSetRequest { account_id, args })
            .map_err(JmapEmailSetError::SerializeArgs)?;

        let mut batch = JmapBatch::new();
        batch.add("Email/set", json_args);
        let request = batch.into_request(vec![CORE_CAPABILITY.into(), MAIL_CAPABILITY.into()]);

        let send = JmapSend::new(http_auth, api_url, request)?;
        Ok(Self {
            state: State::Set(JmapSet::from_send(send)),
        })
    }
}

impl JmapCoroutine for JmapEmailSet {
    type Yield = JmapYield;
    type Return = Result<JmapEmailSetOutput, JmapEmailSetError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> JmapCoroutineState<Self::Yield, Self::Return> {
        trace!("Email/set: {}", self.state);
        match &mut self.state {
            State::Set(set) => {
                let JmapSetOutput {
                    new_state,
                    created,
                    updated,
                    destroyed,
                    not_created,
                    not_updated,
                    not_destroyed,
                    keep_alive,
                } = jmap_try!(set, arg);
                let parse = |map: BTreeMap<String, serde_json::Value>| {
                    map.into_iter()
                        .map(|(k, v)| {
                            let e =
                                serde_json::from_value(v).unwrap_or(JmapEmailSetItemError::Unknown);
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
        }
    }
}

enum State {
    Set(JmapSet<JmapEmail>),
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Set(_) => f.write_str("set"),
        }
    }
}

#[derive(Serialize)]
struct EmailSetRequest {
    #[serde(rename = "accountId")]
    account_id: String,
    #[serde(flatten)]
    args: JmapEmailSetArgs,
}
