//! I/O-free coroutine for the `Mailbox/set` method (RFC 8621 §2.6).

use alloc::{collections::BTreeMap, string::String, vec, vec::Vec};

use io_socket::io::{SocketInput, SocketOutput};
use secrecy::SecretString;
use serde::Serialize;
use thiserror::Error;

use crate::{
    rfc8620::coroutines::send::{JmapBatch, JmapSend, JmapSendError},
    rfc8620::coroutines::set::{JmapSet, JmapSetError, JmapSetResult},
    rfc8620::types::session::JmapSession,
    rfc8620::types::session::capabilities,
    rfc8621::types::mailbox::{Mailbox, MailboxCreate, MailboxUpdate},
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

/// Result returned by the [`JmapMailboxSet`] coroutine.
#[derive(Debug)]
pub enum JmapMailboxSetResult {
    Ok {
        new_state: String,
        created: BTreeMap<String, Mailbox>,
        updated: BTreeMap<String, Option<Mailbox>>,
        destroyed: Vec<String>,
        not_created: BTreeMap<String, crate::rfc8620::types::error::SetError>,
        not_updated: BTreeMap<String, crate::rfc8620::types::error::SetError>,
        not_destroyed: BTreeMap<String, crate::rfc8620::types::error::SetError>,
        keep_alive: bool,
    },
    Io {
        input: SocketInput,
    },
    Err {
        err: JmapMailboxSetError,
    },
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

    /// Makes the coroutine progress.
    pub fn resume(&mut self, arg: Option<SocketOutput>) -> JmapMailboxSetResult {
        match self.set.resume(arg) {
            JmapSetResult::Ok {
                new_state,
                created,
                updated,
                destroyed,
                not_created,
                not_updated,
                not_destroyed,
                keep_alive,
            } => JmapMailboxSetResult::Ok {
                new_state,
                created,
                updated,
                destroyed,
                not_created,
                not_updated,
                not_destroyed,
                keep_alive,
            },
            JmapSetResult::Io { input } => JmapMailboxSetResult::Io { input },
            JmapSetResult::Err { err } => JmapMailboxSetResult::Err { err: err.into() },
        }
    }
}
