//! I/O-free coroutine for the `Mailbox/set` method (RFC 8621 §2.6).

use alloc::{collections::BTreeMap, string::String, vec, vec::Vec};

use secrecy::SecretString;
use serde::Serialize;
use thiserror::Error;

use crate::coroutine::*;
use crate::{
    rfc8620::{send::*, session::JmapSession, set::*},
    rfc8621::{
        capabilities,
        mailbox::{Mailbox, MailboxCreate, MailboxSetError, MailboxUpdate},
    },
};

/// Errors that can occur during the coroutine progression.
#[derive(Debug, Error)]
pub enum JmapMailboxSetError {
    #[error("Send JMAP request error: {0}")]
    Send(#[from] JmapSendError),
    #[error("Serialize Mailbox/set args error: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    #[error("JMAP Mailbox/set error: {0}")]
    Set(#[from] JmapSetError),
}

/// Successful terminal output of [`JmapMailboxSet`].
#[derive(Clone, Debug)]
pub struct JmapMailboxSetOutput {
    pub new_state: String,
    pub created: BTreeMap<String, Mailbox>,
    pub updated: BTreeMap<String, Option<Mailbox>>,
    pub destroyed: Vec<String>,
    pub not_created: BTreeMap<String, MailboxSetError>,
    pub not_updated: BTreeMap<String, MailboxSetError>,
    pub not_destroyed: BTreeMap<String, MailboxSetError>,
    pub keep_alive: bool,
}

/// Arguments for a `Mailbox/set` request.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapMailboxSetArgs {
    /// Objects to create (client ID → partial mailbox object).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create: Option<BTreeMap<String, MailboxCreate>>,

    /// Objects to update (mailbox ID → patch object).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update: Option<BTreeMap<String, MailboxUpdate>>,

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
    args: JmapMailboxSetArgs,
}

/// I/O-free coroutine for the JMAP `Mailbox/set` method.
///
/// Creates, updates, or destroys mailbox objects.
pub struct JmapMailboxSet {
    set: JmapSet<Mailbox>,
}

impl JmapMailboxSet {
    /// Creates a new coroutine.
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        args: JmapMailboxSetArgs,
    ) -> Result<Self, JmapMailboxSetError> {
        let account_id = session
            .primary_accounts
            .get(capabilities::MAIL)
            .cloned()
            .unwrap_or_default();
        let api_url = &session.api_url;

        let json_args = serde_json::to_value(MailboxSetRequest { account_id, args })
            .map_err(JmapMailboxSetError::SerializeArgs)?;

        let mut batch = JmapBatch::new();
        batch.add("Mailbox/set", json_args);
        let request =
            batch.into_request(vec![capabilities::CORE.into(), capabilities::MAIL.into()]);

        let send = JmapSend::new(http_auth, api_url, request)?;
        Ok(Self {
            set: JmapSet::from_send(send),
        })
    }
}

impl JmapCoroutine for JmapMailboxSet {
    type Yield = JmapYield;
    type Return = Result<JmapMailboxSetOutput, JmapMailboxSetError>;

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
                            let e = serde_json::from_value(v).unwrap_or(MailboxSetError::Unknown);
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
            JmapCoroutineState::Complete(Err(err)) => JmapCoroutineState::Complete(Err(err.into())),
            JmapCoroutineState::Yielded(y) => JmapCoroutineState::Yielded(y),
        }
    }
}
