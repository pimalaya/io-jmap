//! JMAP `Mailbox/set` coroutine (RFC 8621 §2.6): wraps the generic [`JmapSet`]
//! with [`JmapMailboxSetArgs`] (create/update/destroy) and decodes per-object
//! [`JmapMailboxSetItemError`] payloads.
//!
//! # Example
//!
//! ```rust,no_run
//! use io_jmap::{
//!     rfc8620::JmapSession,
//!     rfc8621::mailbox::set::{JmapMailboxSet, JmapMailboxSetArgs},
//! };
//! use secrecy::SecretString;
//!
//! # fn demo(session: &JmapSession) {
//! let auth = SecretString::from("Bearer xyz");
//! let args = JmapMailboxSetArgs::default();
//! let coroutine = JmapMailboxSet::new(session, &auth, args).unwrap();
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
        mailbox::{JmapMailbox, JmapMailboxCreate, JmapMailboxSetItemError, JmapMailboxUpdate},
    },
};

/// Failure causes during a JMAP `Mailbox/set` flow.
#[derive(Debug, Error)]
pub enum JmapMailboxSetError {
    #[error("JMAP Mailbox/set failed: {0}")]
    Send(#[from] JmapSendError),
    #[error("JMAP Mailbox/set failed: serialize args: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    #[error("JMAP Mailbox/set failed: {0}")]
    Set(#[from] JmapSetError),
}

/// Arguments for a `Mailbox/set` request.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapMailboxSetArgs {
    /// Objects to create (client ID → partial mailbox object).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create: Option<BTreeMap<String, JmapMailboxCreate>>,

    /// Objects to update (mailbox ID → patch object).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update: Option<BTreeMap<String, JmapMailboxUpdate>>,

    /// IDs of objects to destroy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destroy: Option<Vec<String>>,

    /// Whether to destroy contained emails when destroying a mailbox.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_destroy_remove_emails: Option<bool>,
}

/// Successful terminal output of [`JmapMailboxSet`].
#[derive(Clone, Debug)]
pub struct JmapMailboxSetOutput {
    pub new_state: String,
    pub created: BTreeMap<String, JmapMailbox>,
    pub updated: BTreeMap<String, Option<JmapMailbox>>,
    pub destroyed: Vec<String>,
    pub not_created: BTreeMap<String, JmapMailboxSetItemError>,
    pub not_updated: BTreeMap<String, JmapMailboxSetItemError>,
    pub not_destroyed: BTreeMap<String, JmapMailboxSetItemError>,
    pub keep_alive: bool,
}

/// I/O-free coroutine for the JMAP `Mailbox/set` method.
pub struct JmapMailboxSet {
    state: State,
}

impl JmapMailboxSet {
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        args: JmapMailboxSetArgs,
    ) -> Result<Self, JmapMailboxSetError> {
        let account_id = session
            .primary_accounts
            .get(MAIL_CAPABILITY)
            .cloned()
            .unwrap_or_default();
        let api_url = &session.api_url;

        let json_args = serde_json::to_value(MailboxSetRequest { account_id, args })
            .map_err(JmapMailboxSetError::SerializeArgs)?;

        let mut batch = JmapBatch::new();
        batch.add("Mailbox/set", json_args);
        let request = batch.into_request(vec![CORE_CAPABILITY.into(), MAIL_CAPABILITY.into()]);

        let send = JmapSend::new(http_auth, api_url, request)?;
        Ok(Self {
            state: State::Set(JmapSet::from_send(send)),
        })
    }
}

impl JmapCoroutine for JmapMailboxSet {
    type Yield = JmapYield;
    type Return = Result<JmapMailboxSetOutput, JmapMailboxSetError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> JmapCoroutineState<Self::Yield, Self::Return> {
        trace!("Mailbox/set: {}", self.state);
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
                            let e = serde_json::from_value(v)
                                .unwrap_or(JmapMailboxSetItemError::Unknown);
                            (k, e)
                        })
                        .collect()
                };
                JmapCoroutineState::Complete(Ok(JmapMailboxSetOutput {
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
    Set(JmapSet<JmapMailbox>),
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Set(_) => f.write_str("set"),
        }
    }
}

#[derive(Serialize)]
struct MailboxSetRequest {
    #[serde(rename = "accountId")]
    account_id: String,
    #[serde(flatten)]
    args: JmapMailboxSetArgs,
}
