//! JMAP `Mailbox/get` coroutine (RFC 8621 §2.5): wraps the generic [`JmapGet`]
//! with the JMAP-Mail capability set.
//!
//! # Example
//!
//! ```rust,no_run
//! use io_jmap::{
//!     rfc8620::JmapSession,
//!     rfc8621::mailbox::get::{JmapMailboxGet, JmapMailboxGetOptions},
//! };
//! use secrecy::SecretString;
//!
//! # fn demo(session: &JmapSession) {
//! let auth = SecretString::from("Bearer xyz");
//! let coroutine =
//!     JmapMailboxGet::new(session, &auth, JmapMailboxGetOptions::default()).unwrap();
//! # let _ = coroutine;
//! # }
//! ```

use core::fmt;

use alloc::{borrow::ToOwned, format, string::String, vec, vec::Vec};

use log::trace;
use secrecy::SecretString;
use thiserror::Error;

use crate::{
    coroutine::*,
    jmap_try,
    rfc8620::{CORE_CAPABILITY, JmapSession, get::*},
    rfc8621::{
        MAIL_CAPABILITY,
        mailbox::{JmapMailbox, JmapMailboxProperty},
    },
};

/// Failure causes during a JMAP `Mailbox/get` flow.
#[derive(Debug, Error)]
pub enum JmapMailboxGetError {
    #[error("JMAP Mailbox/get failed: {0}")]
    Get(#[from] JmapGetError),
}

/// Options for [`JmapMailboxGet::new`].
#[derive(Clone, Debug, Default)]
pub struct JmapMailboxGetOptions {
    /// Restrict the fetch to these mailbox IDs; `None` fetches all.
    pub ids: Option<Vec<String>>,
    /// Restrict the returned properties; `None` returns all.
    pub properties: Option<Vec<JmapMailboxProperty>>,
}

/// Successful terminal output of [`JmapMailboxGet`].
#[derive(Clone, Debug)]
pub struct JmapMailboxGetOutput {
    pub mailboxes: Vec<JmapMailbox>,
    pub not_found: Vec<String>,
    pub new_state: String,
    pub keep_alive: bool,
}

/// I/O-free coroutine for the JMAP `Mailbox/get` method.
pub struct JmapMailboxGet {
    state: State,
}

impl JmapMailboxGet {
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        opts: JmapMailboxGetOptions,
    ) -> Result<Self, JmapMailboxGetError> {
        let account_id = session
            .primary_accounts
            .get(MAIL_CAPABILITY)
            .cloned()
            .unwrap_or_default();
        let api_url = &session.api_url;

        let props = opts.properties.map(|ps| {
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
            state: State::Get(JmapGet::new(
                account_id,
                http_auth,
                api_url,
                "Mailbox/get",
                vec![CORE_CAPABILITY.into(), MAIL_CAPABILITY.into()],
                JmapGetOptions {
                    ids: opts.ids,
                    properties: props,
                },
            )?),
        })
    }
}

impl JmapCoroutine for JmapMailboxGet {
    type Yield = JmapYield;
    type Return = Result<JmapMailboxGetOutput, JmapMailboxGetError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> JmapCoroutineState<Self::Yield, Self::Return> {
        trace!("Mailbox/get: {}", self.state);
        match &mut self.state {
            State::Get(get) => {
                let JmapGetOutput {
                    list,
                    not_found,
                    state,
                    keep_alive,
                } = jmap_try!(get, arg);
                JmapCoroutineState::Complete(Ok(JmapMailboxGetOutput {
                    mailboxes: list,
                    not_found,
                    new_state: state,
                    keep_alive,
                }))
            }
        }
    }
}

enum State {
    Get(JmapGet<JmapMailbox>),
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Get(_) => f.write_str("get"),
        }
    }
}
