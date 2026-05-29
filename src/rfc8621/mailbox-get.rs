//! I/O-free coroutine for the standalone `Mailbox/get` method (RFC 8621 §2.5).

use alloc::{borrow::ToOwned, format, string::String, vec, vec::Vec};

use secrecy::SecretString;
use thiserror::Error;

use crate::coroutine::*;
use crate::{
    rfc8620::{get::*, session::JmapSession},
    rfc8621::{
        capabilities,
        mailbox::{Mailbox, MailboxProperty},
    },
};

/// Errors that can occur during the coroutine progression.
#[derive(Debug, Error)]
pub enum JmapMailboxGetError {
    #[error("JMAP Mailbox/get error: {0}")]
    Get(#[from] JmapGetError),
}

/// Successful output of [`JmapMailboxGet`].
#[derive(Clone, Debug)]
pub struct JmapMailboxGetOk {
    pub mailboxes: Vec<Mailbox>,
    pub not_found: Vec<String>,
    pub new_state: String,
    pub keep_alive: bool,
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
}

impl JmapCoroutine for JmapMailboxGet {
    type Output = JmapMailboxGetOk;
    type Error = JmapMailboxGetError;

    fn resume(&mut self, arg: Option<&[u8]>) -> JmapCoroutineState<Self::Output, Self::Error> {
        match self.get.resume(arg) {
            JmapGetResult::Ok {
                list,
                not_found,
                state,
                keep_alive,
            } => JmapCoroutineState::Done(JmapMailboxGetOk {
                mailboxes: list,
                not_found,
                new_state: state,
                keep_alive,
            }),
            JmapGetResult::WantsRead => JmapCoroutineState::WantsRead,
            JmapGetResult::WantsWrite(bytes) => JmapCoroutineState::WantsWrite(bytes),
            JmapGetResult::Err(err) => JmapCoroutineState::Err(err.into()),
        }
    }
}

/// Output of the [`JmapClientStd::mailbox_get`] client method.
///
/// [`JmapClientStd::mailbox_get`]: crate::client::JmapClientStd::mailbox_get
#[derive(Clone, Debug)]
pub struct JmapMailboxGetOutput {
    pub mailboxes: Vec<Mailbox>,
    pub not_found: Vec<String>,
    pub new_state: String,
}
