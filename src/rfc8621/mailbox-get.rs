//! I/O-free coroutine for the standalone `Mailbox/get` method (RFC 8621 §2.5).

use alloc::{borrow::ToOwned, format, string::String, vec, vec::Vec};
use io_socket::io::{SocketInput, SocketOutput};
use secrecy::SecretString;
use thiserror::Error;

use crate::{
    rfc8620::get::{JmapGet, JmapGetError, JmapGetResult},
    rfc8620::session::JmapSession,
    rfc8621::capabilities,
    rfc8621::mailbox::{Mailbox, MailboxProperty},
};

/// Errors that can occur during the coroutine progression.
#[derive(Debug, Error)]
pub enum JmapMailboxGetError {
    #[error("JMAP Mailbox/get error: {0}")]
    Get(#[from] JmapGetError),
}

/// Result returned by the [`JmapMailboxGet`] coroutine.
#[derive(Debug)]
pub enum JmapMailboxGetResult {
    Ok {
        mailboxes: Vec<Mailbox>,
        not_found: Vec<String>,
        new_state: String,
        keep_alive: bool,
    },
    Io {
        input: SocketInput,
    },
    Err {
        err: JmapMailboxGetError,
    },
}

/// I/O-free coroutine for the JMAP `Mailbox/get` method.
///
/// Fetches mailbox objects by ID. Pass `ids: None` to fetch all mailboxes.
pub struct JmapMailboxGet {
    get: JmapGet<Mailbox>,
}

impl JmapMailboxGet {
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        ids: Option<Vec<String>>,
        properties: Option<Vec<MailboxProperty>>,
    ) -> Result<Self, JmapMailboxGetError> {
        let account_id = session
            .primary_accounts
            .get(capabilities::MAIL)
            .cloned()
            .unwrap_or_default();
        let api_url = &session.api_url;

        let props = properties.map(|ps| {
            ps.iter()
                .map(|p| {
                    serde_json::to_value(p)
                        .ok()
                        .and_then(|v| v.as_str().map(str::to_owned))
                        .unwrap_or_else(|| format!("{p:?}"))
                })
                .collect()
        });

        Ok(Self {
            get: JmapGet::new(
                account_id,
                http_auth,
                api_url,
                "Mailbox/get",
                vec![capabilities::CORE.into(), capabilities::MAIL.into()],
                ids,
                props,
            )?,
        })
    }

    pub fn resume(&mut self, arg: Option<SocketOutput>) -> JmapMailboxGetResult {
        match self.get.resume(arg) {
            JmapGetResult::Ok {
                list,
                not_found,
                state,
                keep_alive,
            } => JmapMailboxGetResult::Ok {
                mailboxes: list,
                not_found,
                new_state: state,
                keep_alive,
            },
            JmapGetResult::Io { input } => JmapMailboxGetResult::Io { input },
            JmapGetResult::Err { err } => JmapMailboxGetResult::Err { err: err.into() },
        }
    }
}
