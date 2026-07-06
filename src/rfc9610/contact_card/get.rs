//! JMAP `ContactCard/get` coroutine (RFC 9610 §3.1): wraps the generic
//! [`JmapGet`] with the JMAP-Contacts capability set.
//!
//! # Example
//!
//! ```rust,no_run
//! use std::{
//!     io::{Read, Write},
//!     net::TcpStream,
//! };
//!
//! use io_jmap::{
//!     coroutine::{JmapCoroutine, JmapCoroutineState, JmapYield},
//!     rfc8620::JmapSession,
//!     rfc9610::contact_card::get::{JmapContactCardGet, JmapContactCardGetOptions},
//! };
//! use secrecy::SecretString;
//!
//! // Ready stream needed (TCP-connected, TLS-negociated)
//! let mut stream = TcpStream::connect("api.example.com:443").unwrap();
//! let mut buf = [0u8; 4096];
//!
//! let session: JmapSession = serde_json::from_str(r#"{
//!     "username": "",
//!     "accounts": {},
//!     "primaryAccounts": {"urn:ietf:params:jmap:contacts": "a1"},
//!     "capabilities": {},
//!     "apiUrl": "https://api.example.com/jmap/",
//!     "downloadUrl": "",
//!     "uploadUrl": "",
//!     "eventSourceUrl": "",
//!     "state": ""
//! }"#).unwrap();
//! let auth = SecretString::from("Bearer xyz");
//! let mut coroutine =
//!     JmapContactCardGet::new(&session, &auth, JmapContactCardGetOptions::default()).unwrap();
//! let mut arg = None;
//!
//! let out = loop {
//!     match coroutine.resume(arg.take()) {
//!         JmapCoroutineState::Yielded(JmapYield::WantsWrite(bytes)) => {
//!             stream.write_all(&bytes).unwrap();
//!         }
//!         JmapCoroutineState::Yielded(JmapYield::WantsRead) => {
//!             let n = stream.read(&mut buf).unwrap();
//!             arg = Some(&buf[..n]);
//!         }
//!         JmapCoroutineState::Complete(Ok(out)) => break out,
//!         JmapCoroutineState::Complete(Err(err)) => panic!("{err}"),
//!     }
//! };
//!
//! println!("{} cards", out.cards.len());
//! ```

use core::fmt;

use alloc::{string::String, vec, vec::Vec};

use log::trace;
use secrecy::SecretString;
use thiserror::Error;

use crate::{
    coroutine::*,
    jmap_try,
    rfc8620::{CORE_CAPABILITY, JmapSession, get::*},
    rfc9610::{CONTACTS_CAPABILITY, contact_card::JmapContactCard},
};

/// Failure causes during a JMAP `ContactCard/get` flow.
#[derive(Debug, Error)]
pub enum JmapContactCardGetError {
    #[error("JMAP ContactCard/get failed: {0}")]
    Get(#[from] JmapGetError),
}

/// Options for [`JmapContactCardGet::new`].
#[derive(Clone, Debug, Default)]
pub struct JmapContactCardGetOptions {
    /// Restrict the fetch to these ContactCard IDs; `None` fetches all.
    pub ids: Option<Vec<String>>,
    /// Restrict the returned properties (JSContact property names plus `id`
    /// and `addressBookIds`); `None` returns all.
    pub properties: Option<Vec<String>>,
}

/// Successful terminal output of [`JmapContactCardGet`].
#[derive(Clone, Debug)]
pub struct JmapContactCardGetOutput {
    pub cards: Vec<JmapContactCard>,
    pub not_found: Vec<String>,
    pub new_state: String,
    pub keep_alive: bool,
}

/// I/O-free coroutine for the JMAP `ContactCard/get` method.
pub struct JmapContactCardGet {
    state: State,
}

impl JmapContactCardGet {
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        opts: JmapContactCardGetOptions,
    ) -> Result<Self, JmapContactCardGetError> {
        let account_id = session
            .primary_accounts
            .get(CONTACTS_CAPABILITY)
            .cloned()
            .unwrap_or_default();
        let api_url = &session.api_url;

        Ok(Self {
            state: State::Get(JmapGet::new(
                account_id,
                http_auth,
                api_url,
                "ContactCard/get",
                vec![CORE_CAPABILITY.into(), CONTACTS_CAPABILITY.into()],
                JmapGetOptions {
                    ids: opts.ids,
                    properties: opts.properties,
                },
            )?),
        })
    }
}

impl JmapCoroutine for JmapContactCardGet {
    type Yield = JmapYield;
    type Return = Result<JmapContactCardGetOutput, JmapContactCardGetError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> JmapCoroutineState<Self::Yield, Self::Return> {
        trace!("ContactCard/get: {}", self.state);
        match &mut self.state {
            State::Get(get) => {
                let JmapGetOutput {
                    list,
                    not_found,
                    state,
                    keep_alive,
                } = jmap_try!(get, arg);
                JmapCoroutineState::Complete(Ok(JmapContactCardGetOutput {
                    cards: list,
                    not_found,
                    new_state: state,
                    keep_alive,
                }))
            }
        }
    }
}

enum State {
    Get(JmapGet<JmapContactCard>),
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Get(_) => f.write_str("get"),
        }
    }
}
